# B0117 — animation frame cost under the 120 Hz budget

Status: review candidate. Follows B0110 ([0110-animation-smoothness.md](0110-animation-smoothness.md)): the joined-frame
artwork was still rasterised for every reveal frame (5–75 ms on the desktop), so the dashboard, control centre and launcher
cost 13–30 ms per frame against an 8.3 ms budget at 120 Hz. Owner rule SMOOTHNESS FIRST (2026-10-25): memory may be spent.

## Findings (profiled with `BEAD_PROFILE_INPUT=1` stage spans, desktop, 2880×1800 at 1.5, CPU renderer)

1. **The cost was one call.** `reveal_art_raster` (resvg of the contour SVG through the tree renderer) took 14 ms for
   the smallest dashboard crop and up to 75 ms for the largest; SVG generation (`reveal_art_draw`) took 0.05 ms and
   compositing (`compose`) 0.3–0.7 ms. It also ran again for every frame at an unchanged pose, including the long
   tail of a closing panel (progress 1e-6 and below, a sub-pixel panel).
2. **Reveal curves start slowly.** The first six dashboard frames sit below 12 % progress, so a fix only for large
   reveals leaves the first ~100 ms at 14 ms per frame; the closing tail is the same.
3. **Compositing blended every raster through tiny-skia's masked `draw_pixmap`** (17 ms for a full-size artwork
   repaint) although the mask is only a rectangle.
4. **Decoration tiles were re-rasterised on the main thread** at every open and close (`decorations_sync`
   10–21 ms, in the event handler right at the animation start).
5. **A clip-only change damaged the whole clipped item** (old ∪ new bounds) instead of the strip between the two clip edges.
6. **Shadows of panels without joined art** (the AI sidebar app) were regenerated per frame (up to 25 ms).
7. Not a cost: text re-shaping, image scaling and per-frame clears are already ≤ 0.5 ms in steady reveal frames
   (`paint_*` stages, `present_prepare` 0.03 ms median after the change).

## Changes

- `bead-render/src/tree.rs` — **growing-edge template.** The open-state artwork (already prepared by the worker) is the
  frame; it is clipped above the growing edge and a band of the edge (≈ 52 logical px: corner radius plus rim, ≈ 0.7 MiB
  for the dashboard) is slid over it by whole physical pixels. The band is rasterised once per (artwork, scale, body,
  edge) when the prepared frame arrives (`reveal_edge_build`, 5–10 ms, off the animation), at a ghost progress clear of
  the origin join, and **validated against the artwork's own edge** (uniform interior row, every pixel within the
  neighbouring artwork pixels ± 28); a template that does not match is dropped and the piece is drawn the old way.
  Works for all four edges; the same mechanism slides the nine-patch shadow of panels without art (`EdgeKind::Shadow`).
- Small reveals (< 84 logical px) draw both ends of the artwork and stretch one drawn line across the uniform middle
  (`sliced_art`, 2–5 ms instead of 14 ms); a panel under one pixel draws a blank pixel.
- `RevealArt::draw(revealed, crop)`: the closure can draw an exact crop (placement.rs maps it to the contour SVG).
- An unchanged reveal pose (prepared frame key, progress, scale) reuses the derived frame.
- Raster items are blended straight into the damage strip (`blend_over`): opaque and empty pixels short-circuit, partial
  alpha is computed exactly as tiny-skia's float pipeline (unit-tested bit for bit; the launcher golden stays exact).
- `damage.rs`: a changed clip edge damages only the strip between the old and new edge (`clip_difference`).
- `bead-wayland/decorations.rs`: rasterised decoration tile sets are cached per key (idle frame, joined frame of a
  panel), four sets, ≤ 2 MiB each; close restores the idle set with a memcpy.
- `nacre-shell/reload.rs`: `BACKEND_UPDATE` is printed only with `BEAD_VERBOSE_LOG` (owner request; B0119 already stops a
  title-only snapshot from reaching the bar's model or the log).
- `BEAD_APP_LOG_DIR` keeps an app process's (sidebar) profile output for the harness; `UI_CALLBACK` joins `UI_FRAME` under
  `BEAD_FRAME_TRACE`.
- `spikes/animation-pacing.py`: (a) a commit whose only damage is one pixel, arriving after 100 ms of quiet, is housekeeping
  and no animation frame (Bead sends one after a panel closes to get presentation feedback; QML never does — it showed as a
  220 ms "drop"); (b) the report medians include windows without any dropped frame (a zero used to be treated as missing,
  which printed the median of the *non-zero* windows only); (c) `rescore DIR ARM` recomputes an earlier run with the
  current metric. Numbers below use (a)–(c) for every arm, before and after.

## Results

Private nested session on the desktop (12 cores, powersave governor), 2880×1800 at 1.5, CPU renderers for Bead, QML on
its own threaded OpenGL, desktop lock per run, swap 0, **full-LTO release builds** of origin/main (before) and of this
change (after), medians of 3 repeats. Full tables: [evidence/0117](evidence/0117/) (`pacing-60hz.txt`,
`pacing-120hz.txt`). `drop%` = missed vsyncs / expected frames, `p99` = commit-interval p99 in ms, `rndr99` = p99 of the
CPU time one frame took in the shell (the 120 Hz budget is 8.3 ms), `slow%` = frames costing more than one vsync.

**Stage cost per frame, dashboard** (desktop, profile on, ms): `present_prepare` p50 14.3 / p99 78.6 before → p50 0.04 /
p99 15.5 after (the 15 ms is the once-per-size template build, now at frame prepare); `compose` p99 31.3 → 14.1 (the tab
content repaint of the harness, see gaps).

**60 Hz, QML vs Bead before / after** (drop% / p99 ms):

| Animation | QML | Bead before | Bead after |
| --- | ---: | ---: | ---: |
| control centre open | 5.6 / 32.0 | 5.3 / 34.9 | 10.5 / 33.7 |
| control centre close | 0 / 19.8 | 0 / 18.5 | 0 / 23.9 |
| dashboard open | 4.3 / 32.0 | 29.2 / 32.3 | 16.0 / 32.4 |
| dashboard close | 4.3 / 25.2 | 20.8 / 34.5 | **0 / 20.9** |
| launcher open | 28.1 / 136.1 | 6.2 / 38.2 | **5.0 / 33.4** |
| launcher close | 4.8 / 27.1 | 4.8 / 26.1 | **0 / 17.7** |
| popout network open / close | 0 / 19.4, 0 / 17.7 | 6.2 / 32.5, 0 / 23.1 | 7.1 / 32.3, 5.9 / 31.1 |
| popout audio open / close | 0 / 19.7, 0 / 18.7 | 6.2 / 31.4, 0 / 18.3 | 6.2 / 33.6, 5.9 / 32.2 |
| hover peek open / close | 2.1 / 31.2, 4.3 / 27.1 | 0 / 23.1, 20.8 / 33.0 | 0 / 20.1, **0 / 21.1** |
| sidebar open / close | 4.0 / 31.9, 0 / 17.2 | 0 / 22.7, 0 / 16.9 | **3.8 / 29.1**, 0 / 21.2 |

**120 Hz** (`PACING_HZ=120`, parent output at 120 Hz). The nested niri now gives a **real 120 Hz frame clock** to Bead
(commit-interval p50 7.8-8.1 ms in every window; the B0110 note that it paces clients at 60 fps no longer holds for Bead),
while QML still produces ~60 fps (it misses 37-54 % of the 120 Hz vsyncs in every animation). Missed 120 Hz vsyncs
(drop%) and CPU frame cost (`rndr99` ms / `slow%`, before → after; QML is not instrumented for cost):

| Animation | QML drop% | Bead before drop% | Bead after drop% | rndr99 / slow% before → after |
| --- | ---: | ---: | ---: | ---: |
| control centre open / close | 44.1 / 47.1 | 32.4 / 8.1 | 8.3 / 0 | 14.0 / 73 → **5.1 / 0**, 13.2 / 26 → 5.7 / 0 |
| dashboard open / close | 44.2 / 46.5 | 47.7 / 42.6 | 8.2 / 2.2 | 29.1 / 52 → 9.7 / 2.2, 30.9 / 38 → 11.1 / 2.1 |
| launcher open / close | 54.0 / 37.1 | 55.3 / 13.5 | 7.7 / 0 | 23.2 / 78 → **3.5 / 0**, 23.2 / 27 → 4.8 / 0 |
| popout network open / close | 46.4 / 50.0 | 10.0 / 0 | 6.7 / 3.2 | 7.6 → 2.2, 8.6 → 2.4 |
| popout audio open / close | 41.9 / 50.0 | 28.1 / 21.9 | 3.2 / 3.2 | 14.3 / 42 → 2.6 / 0, 15.8 / 35 → 3.3 / 0 |
| hover peek open / close | 47.3 / 47.7 | 45.5 / 41.3 | 22.4 / 2.2 | 30.2 / 22 → 10.1 / 1.3, 30.8 / 35 → 11.1 / 2.0 |
| sidebar open / close | 47.9 / 48.0 | 28.0 / 22.6 | 5.9 / 1.9 | 4.7 → 0.6, 4.5 → 0.5 |

Bead's missed vsyncs are below QML's in every row at 120 Hz. The hover-peek open's 22 % is one 82 ms interval in one of
three runs (median of the three windows).

**Launcher first key** (UI CPU ms, input receipt to presentation, 3 alternating pairs, [first-key.txt](evidence/0117/first-key.txt)):
standalone control 5.70 / 5.80 / 5.90, candidate 5.66 / 5.74 / 5.76; bar Home lane control 6.17 / 6.53 / 6.65, candidate
6.36 / 6.50 / 6.59. Gate ≤ 8 ms met, unchanged within 0.3 ms.

**Gates.** p99 frame cost ≤ 8.3 ms: **met** for control centre, launcher, both popouts and the sidebar; the dashboard
(9.7 / 11.1) and hover peek (≈ 10–11) are above it only because of the one content-repaint frame per window (below).
"Bead drop% ≤ QML for every animation": **met** for dashboard close, launcher open/close, hover peek, sidebar open/close,
control-centre close; **not met** for dashboard open (16 vs 4.3), control-centre open (10.5 vs 5.6) and the two popouts
(≈ 6 vs 0). These are one or two 32 ms intervals per window, no longer CPU cost: see the next section.

## Why the remaining drops are not frame cost

The wl_surface/presentation trace of a dashboard open (`WAYLAND_DEBUG=client`) shows the commits at 29, 61, 93 and 125 ms
each 30 ms apart because **the nested niri presents them 32 ms apart** (`presented` timestamps +32.0, +32.1, +16.0 ms), then
returns to 16 ms; our frame cost in those frames is 2–4 ms and every commit follows its `wl_callback.done` within 3 ms.
It is the first ~100 ms after a panel surface maps: new shm buffers, the freshly created decoration subsurfaces and a
software-composited (pixman) 2880×1800 output. QML pays one such interval per window, Bead two to four. Not attributable
to the shell's frame work, so not addressed here; a niri-side (or buffer-priming) experiment is the follow-up. Candidate
levers tried and rejected by data: pixel-identical early frames (only 5 of 22 late callbacks coincide with an empty damage
commit), damage size (157-row and 180-row frames behave differently), render time (2–4 ms).

## Memory cost (SMOOTHNESS FIRST: spent, recorded)

Shell process after each phase, closed 0.8 s after the close, one run per phase of the same sequence, CPU renderer
([memory-closed-warm.txt](evidence/0117/memory-closed-warm.txt)): anonymous +0.4 to +2.1 MiB (idle 18.2 → 18.8, dashboard
32.2 → 32.6, launcher 30.7 → 32.4, popout audio 29.2 → 31.3, sidebar 33.3 → 34.7), shared memory +0 to +2.1 MiB (control
centre 5.7 → 7.8). Sources: the edge template (≈ 0.7 MiB per size), cached decoration tile sets (≤ 2 MiB each, four), the
derived reveal frame kept for the current pose. The +0.5 MiB median gate would not pass on the anonymous side; the change
removes jank, so it is reported rather than gated (only single runs per phase; no five-pair median was taken).

## Known gaps

- Items above: drop% parity for dashboard open, control-centre open and the popouts is a compositor-side cadence (above).
- Dashboard / hover-peek: one 10–15 ms frame per window when content changes (the harness switches a tab right after
  opening, and the close refreshes it). `paint_rounded` ≈ 5 ms and `paint_image` ≈ 3.5 ms for 1.2 Mpx; an interior fast path
  for translucent rounded fills would remove most of it.
- Sidebar: the app process still starts cold on first use (worker bake 50–80 ms, first frame ≈ 125 ms); the process is
  kept 120 s after a close. A hidden pre-spawned process was not built (memory of a standing process; larger change).
- Template build and the first full content paint (5–12 ms each) remain once per panel size and open.
- GL panels: the CPU path now reaches the budget for every panel except the content-repaint frame, so the GL renderer
  was not re-measured (B0110: no gain on the desktop's AMD; the laptop's Intel xe is still unmeasured).
- Measurement caveat: the desktop CPU runs the `powersave` governor; the same resvg call took 2 ms in a tight loop and
  12–14 ms in the animation, so absolute milliseconds here are pessimistic for a sustained load.

## Temporary duplicates and expiry

The per-frame SVG path (`reveal_art_raster` through the tree renderer) stays as the fallback when no valid template
exists (small reveals sliced, unusual geometry). It is a fallback, not a legacy copy; no expiry.

## Evidence

[evidence/0117](evidence/0117/): `pacing-60hz.txt`, `pacing-120hz.txt` (QML / before / after), `memory-closed-warm.txt`,
`first-key.txt`. Raw runs stay on the desktop under `~/nacre-lead/b0117/evidence` until cleaned.
