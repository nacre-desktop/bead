# B0110 — animation smoothness audit and production log spam

Status: review candidate. Owner report (live 4f8db4c, Lunar Lake, 2880×1800 at 1.5, niri): animations are
"choppy, not entirely smooth"; the control panel "sometimes opens dark without anything in it and the content
pops in after it is open, or opens dark and then re-opens correctly". Owner rule SMOOTHNESS FIRST (2026-10-25):
smoothness beats low memory; warm/pre-built panels are allowed, record the cost.

## Findings (profiled, not guessed)

Harness: `spikes/animation-pacing.py` (below). Frame cost comes from `BEAD_PROFILE_INPUT=1` stage spans.

1. **Blank first frame, reveal clock spent on nothing.** `start_panel` creates the panel runtime on open. The
   worker needs 35–60 ms (`WORKER_PREPARE reload=true`) to produce the first scene, yet the surface drew the
   blank fallback at once (first buffer 12 ms after the trigger, first content commit 36–60 ms) and the reveal
   animation already ran against it. That is the dark/empty first frame and the "content pops in" of the
   control panel, dashboard, launcher, popouts and sidebar. The animation clock also advanced during the
   wait, so the first content frames were already mid-animation (dashboard open: 34 % dropped, longest gap 122 ms).
2. **Per-frame shadow template.** Every reveal frame re-shaped the panel shadow and re-rendered its Gaussian
   template SVG (radius/blur/scale only) from scratch (`shadow_pixmap`): up to 29 ms per frame.
3. **Per-frame opacity composite.** The fade path allocated a full-surface `Mask` and ran tiny-skia's masked
   `draw_pixmap` every frame.
4. **Per-notch glide on a fixed 16 ms timer; no touchpad fling.** 120 Hz outputs received a scroll position every
   second frame, and a touchpad lift stopped dead where QML's Flickable coasts.
5. **Icon lookup on every scene.** `freedesktop_icons::lookup` (directory stat walk, no cache) ran for every
   icon of every launcher scene; large raster icons were downscaled with Lanczos3 (tens of ms each).
6. **Log spam.** `PROFILE`, `UI_COMMIT`, `UI_RESOURCES`, `UI_PRESENTED`, `BUFFER`, `DAMAGE`, … printed per
   frame/commit in production.
7. **Not changed, measured:** every core panel is created with the CPU renderer on purpose (driver residency).
   Forcing `--panel-renderer femtovg-gl` (AMD radeonsi on the desktop; not Intel) did not lower the dashboard
   render p99 meaningfully (30.7 → 26.4 ms) and made the launcher's first open worse (render p99 79 ms for
   the GL warm-up), so the default stays CPU. Intel Lunar Lake is not available to this task: the opt-in flag
   remains for the owner to try live.
8. **Residual (open):** the joined-frame artwork (`RevealArt`, placement.rs) is regenerated as an SVG and
   rasterised for every reveal frame (5–20 ms on dashboard/launcher/control). Next step if the gate is
   still missed: pre-rasterise the reveal frames on the worker while the content is prepared (memory ≈ one
   trimmed frame per step, allowed by the new rule).

## Changes

- `crates/bead-core/src/profile.rs`: `profile::verbose()` and `vprintln!`. Per-frame diagnostics print only
  with `BEAD_VERBOSE_LOG=1` (or any `BEAD_PROFILE_INPUT`); nothing is written per frame in normal use. Harness
  scripts under `spikes/` set `BEAD_VERBOSE_LOG=1` themselves. One-shot lines (`READY`, `STOP`, errors) stay.
  `BEAD_FRAME_TRACE=1` adds `UI_FRAME monotonic_ns render_ms companion` per rendered frame (harness only).
- `bead-wayland` panel open: **no frame is committed for a panel until its own scene exists** (bounded at
  750 ms so a failing worker still shows its error scene); the reveal clock restarts when that first scene
  arrives (`panel_content_ready`). A first frame that took >4 ms to build restarts its motion
  (`Animations::rebase_fresh`) so baking never spends the animation's opening.
- A closed panel stays built for 120 s (was 2.2 s) so a repeat open shows prepared content on its first
  frame (`PANEL_KEEP_ALIVE`). Memory cost: one panel runtime (Luau VM ≈ 1.2 MiB, layers, SHM) stays resident
  after a close; see the memory table.
- `bead-render`: shadow Gaussian template memoised by (radius, blur, scale), 6 entries (~0.2 MiB each);
  opacity composite replaced by a masked-free scale blit with the BGRA swap fused (`blit_with_opacity`,
  tested against tiny-skia within ±2); large raster icons use CatmullRom instead of Lanczos3.
- `nacre-shell`: icon name → path memo (found paths only, revalidated by one stat).
- `bead-wayland/scroll.rs`: glide and fling ticks follow the output refresh (presentation feedback),
  touchpad release produces a decelerating fling with Qt Flickable constants (1500 px/s², ≤2500 px/s, 65 ms
  release), unit-tested.
- `spikes/virtual-pointer.c`: `stream` mode (one device, absolute deadlines) for scroll gestures.

## Harness

`spikes/animation-pacing.py` wraps `parity-compare.py` (same private headless labwc → niri → shell topology,
fixtures, QML and Bead arms). Both shells run with `WAYLAND_DEBUG=client`; the client protocol trace gives
every buffer-bearing `wl_surface.commit` for QML and Bead alike. Bead adds `UI_FRAME` (render ms), `UI_COMMIT`
(first content-bearing scene) and wp_presentation `presented` stamps. Metrics per animation window: commit
interval p50/p95/p99, dropped frames (interval > 1.5 vsync, counted in missed vsyncs), longest gap, first-frame
latency, first content commit, share of frames whose render exceeds one vsync, shell memory components.
`PACING_HZ=60|120` sets the private parent output refresh. `animation-pacing.py report label=DIR:ARM …`
prints the comparison table.

Caveat that applies to every number: niri 26.04 nested advertises 60 Hz to clients regardless of its parent
(see B0105), so the "120 Hz" parent produces the same ~60 fps client cadence for QML and Bead. Frame **cost**
(`rndr99`, `slow%` = frames costing more than one vsync) is the reliable 120 Hz indicator: 8.3 ms budget.

## Results

RESULTS_PLACEHOLDER

## Temporary duplicates and expiry

None added. `PANEL_KEEP_ALIVE` (120 s) is a tuning constant, not a legacy path.

## Known gaps / next

- Reveal artwork raster per frame (finding 8).
- GL panels on Intel not measurable here; opt-in `--panel-renderer femtovg-gl` kept.
- Launcher first key ≤ 8 ms: not re-measured by this unit (B0039 baseline 12.8 ms UI CPU); the scene-prep
  savings above help it but the gate is unverified.
- Wallpaper carousel could not be driven on the private daemon (`wallpaper.catalogue` unknown); not measured.
