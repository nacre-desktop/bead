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

Private nested session on the desktop (12 cores), 2880×1800 at 1.5, CPU renderers for Bead (QML on its own
threaded OpenGL), desktop lock held per run, swap 0. Medians of 3 repeats; full tables in
[evidence/0110](evidence/0110/): `pacing-60hz.txt`, `pacing-120hz.txt`, `first-visible-vs-content.txt`.
Before = origin/main + instrumentation only; after = this change. `drop%` = missed vsyncs / expected frames,
`p99` = commit-interval p99 (ms), `slow%` = Bead frames whose render cost exceeds one vsync (Bead only).

**First visible frame versus first content-bearing frame** (ms after the trigger, 60 Hz; first buffer-bearing commit
and first panel-scene commit). Before: the panel was shown 13–35 ms before its content existed.

| Panel open | before: visible / content | after: visible / content |
| --- | ---: | ---: |
| control centre | 14.5 / 49.3 | 18.6 / 19.7 |
| dashboard | 18.2 / 61.1 | 21.8 / 22.5 |
| launcher | 14.8 / 48.8 | 19.8 / 20.3 |
| popout network | 14.4 / 36.2 | 17.3 / 17.5 |

After the change a panel's first visible frame is its first content frame (within 1.1 ms), so no dark or empty
frame and no re-open. Popout audio (first visible 62.8 ms) and the sidebar (no panel scene commit: the sidebar is a
separate app process) were not brought to the same bound; see gaps.

**60 Hz, QML versus Bead before / after** (drop% / p99 ms):

| Animation | QML | Bead before | Bead after |
| --- | ---: | ---: | ---: |
| control centre open | 11.1 / 32.1 | 9.1 / 36.4 | **5.3 / 34.9** |
| control centre close | 5.6 / 26.0 | 0 / 21.6 | 0 / 17.4 |
| dashboard open | 4.3 / 29.7 | 32.5 / 122.6 | 26.1 / 32.1 |
| dashboard close | 4.3 / 27.2 | 22.6 / 45.0 | 28.6 / 48.4 |
| launcher open | 28.1 / 137.4 | 7.7 / 36.9 | **6.2 / 37.5** |
| launcher close | 4.8 / 25.1 | 0 / 24.2 | 4.8 / 26.2 |
| popout network open | 6.2 / 18.2 | 4.0 / 31.9 | 6.2 / 32.3 |
| popout audio open | 0 / 18.2 | 7.4 / 31.7 | 6.2 / 32.0 |
| sidebar open | 4.2 / 31.9 | 2.6 / 35.4 | 11.5 / 22.3 |
| hover peek open / close | 2.2 / 4.3 | 0 / 19.2 | 4.2 / 24.0 |

Dashboard open: the 122 ms gap is gone (p99 32 ms) and the rate fell from 32.5 % to 26.1 % but is still
above QML's 4.3 %; dashboard close and the hover-peek close are unchanged or slightly worse (noise ±5 %).
**The "Bead drop rate ≤ QML for every animation" gate is NOT met** for dashboard open/close, hover-peek close and
the sidebar open; it is met for control centre, launcher, popout network and audio. At 120 Hz the nested niri
still paces clients at ~60 fps for both shells (QML drops ≈45 % of 120 Hz vsyncs everywhere); Bead's frame cost
is the limit there: dashboard/hover-peek frames cost ≈30 ms (`slow%` 36–52 %), control centre 13 ms, launcher 23 ms.
The 8.3 ms 120 Hz budget is not met by the CPU renderer for any large panel; closing that needs the residual in
finding 8 (or the GL panel path on the owner's Intel GPU, which could not be measured here).

Settings scroll (open window included): wheel and touchpad drive 20–50 % missed vsyncs in the first second because
the Settings window itself opens (first frame 119–135 ms); the scroll cadence after the change follows the output
refresh. A QML scroll comparison was not obtained (the QML arm ran before the stream injector existed).

**Memory cost of keeping panels warm** (shell process, closed state 0.8 s after close, 3 runs each, median):

| Phase | Anon before → after | Shmem before → after |
| --- | ---: | ---: |
| idle | 17.6 → 17.6 MiB | 0.7 → 0.7 |
| after launcher | 18.0 → 32.0 (+14.0) | 0.7 → 7.7 (+7.0) |
| after dashboard | 18.6 → 31.5 (+12.9) | 0.7 → 7.7 (+7.0) |
| after control centre | 19.3 → 29.2 (+9.9) | 0.7 → 5.7 (+5.0) |

That is +10–14 MiB anonymous and +5–7 MiB shared memory while a panel is held warm (120 s after the last close),
well above the old +0.5 MiB median gate; accepted under SMOOTHNESS FIRST and recorded for the lead. Lowering
`PANEL_KEEP_ALIVE` trades it back. Idle (never opened) memory is unchanged.

**Log spam:** all per-frame/per-commit print sites (`PROFILE`, `UI_COMMIT`, `UI_RESOURCES`, `UI_PLATFORM`, `UI_PRESENTED`,
`BUFFER`, `RESOURCES`, `DAMAGE`, `UI_STATE`, `UI_EVENT`, `PANEL_FRAME`, …) go through `vprintln!`; one-shot `READY`/`STOP`/error lines
remain. Verified by reading the sites and by the harnesses still working with the variable set; a quiet run of the
released binary was not separately counted.

## Temporary duplicates and expiry

None added. `PANEL_KEEP_ALIVE` (120 s) is a tuning constant, not a legacy path.

## Known gaps / next

- Reveal artwork raster per frame (finding 8).
- GL panels on Intel not measurable here; opt-in `--panel-renderer femtovg-gl` kept.
- Launcher first key ≤ 8 ms: not re-measured by this unit (B0039 baseline 12.8 ms UI CPU); the scene-prep
  savings above help it but the gate is unverified.
- Wallpaper carousel could not be driven on the private daemon (`wallpaper.catalogue` unknown); not measured.
