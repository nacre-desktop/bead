# B0185: per-frame render cost of panel slides

Goal: every panel slide at the display rate on the owner's laptop (2880x1800 @ 120 Hz, scale 1.5,
CPU panel renderer). The live audit of 6f6bde99 showed the dashboard (27–29 ms gaps), launcher
(22–26 ms) and control centre (23 ms) far below 120 fps while the status dropdowns, calendar and
power menu already ran at ~120 fps.

## Cause

A slide frame on the UI thread (`draw_current`) is: pose the scene, `prepare_on_ui`, semantic
damage, the CPU repaint of the owed regions, attach/commit. Measured per step with
`frame_cost_bench` (below), the repaint dominates, and it is a full repaint of the panel:

- Closing, the whole open picture moves, so the whole surface is damaged every frame; opening,
  the launcher and control centre slide their content behind the reveal window (the dashboard
  and popouts only grow their body), so again the whole body is damaged.
- The CPU renderer's `Direct` composition then repaints every content primitive into that region
  each frame: tiny-skia fills, strokes and blends of every card, row and border at 1.5x (the
  `fill_path`/`stroke_path`/`blit` share of the live profile). The cost scales with surface area
  times content density: 15.6 ms per dashboard close frame and 6.6 ms per control-centre frame on
  a dev-2 core, versus 0.03–0.07 ms for the small calendar dropdown.

`prepare_on_ui` and damage are small next to it (table below).

## Fix

One rule in `bead-render`'s `TreeCpuRenderer::render`: while a reveal panel slides
(`motion.reveal && progress < 1`), its content moves as one piece, so it is flattened once per
slide into a layer and each frame composites it.

- `SlideContent::of(scene, posed frame)` splits the posed frame: opening, the body (motion parts,
  their growing-edge pieces and anything below them) stays primitive-painted because it reshapes
  per frame; everything above it is content. Closing, the whole scene is content. A guard checks
  the posed items map onto the scene's primitives; otherwise the frame paints as before.
- The layer (scene bounds at the target scale, own clips baked in, rasters reused from the posed
  frame) is painted synchronously on the slide's first frame on the UI thread, keyed by the
  content's primitives, clips, raster keys, scale and format. Nothing is prepared ahead or off
  thread, so the first frame cannot be late or show content arriving mid-slide (the B0177/B0180
  failure modes); its first-frame cost is one content paint, the same work every frame did before.
- Each frame paints the body items, then blends the layer at its pose (content offset, rounded to
  device pixels) clipped to the damage and the reveal window.
- The layer is dropped on the first non-slide frame. Memory: one premultiplied RGBA layer of the
  panel bounds while a slide runs (dashboard ~6.7 MB), charged to the existing 64 MiB
  `cpu-composite` budget; nothing stays resident when idle.

`paint_with_scratch` now delegates to `paint_items(&[Item], ..)`, so the body and the layer reuse
the existing painter. B0175's pose preparation (no open-pose flash), B0178's committed-frame
clock and B0181's close poses are unchanged: only how the posed frame is painted changed.

General engine primitive: slide content layers in the CPU tree renderer (no Nacre ids).

## Evidence

Per-frame UI-thread cost (`frame_cost_bench`, release, scale 1.5, 5 slides per row, owed regions
= current + previous frame damage):

Host: dev-2 container (8 CPUs), `cargo test --release` with `LTO=off, codegen-units=16`
overrides (same dependency opt-levels), baseline 6f6bde99 vs candidate, run back to back.
Pose (`panel_offset`, incl. reveal artwork) ≤ 0.03 ms, `prepare_on_ui` ≤ 0.03 ms and damage
≤ 0.03 ms per frame in both builds; the repaint is the cost:

| Panel (surface px) | dir | owed share | paint p50 before | after | total p95 before | after |
|---|---|---|---|---|---|---|
| dashboard (1572x1071) | open | 0.23 | 1.75 ms | 0.22 ms | 2.94 | 2.92 |
| dashboard | close | 1.90 | 15.58 ms | 1.06 ms | 15.79 | 1.81 |
| launcher (1746x981) | open | 1.54 | 2.37 ms | 1.23 ms | 2.74 | 1.89 |
| launcher | close | 1.91 | 2.50 ms | 1.21 ms | 2.68 | 1.34 |
| control centre (732x1206) | open | 1.52 | 6.60 ms | 1.43 ms | 7.06 | 3.59 |
| control centre | close | 1.84 | 6.59 ms | 0.63 ms | 6.88 | 0.93 |
| calendar popout (561x564) | open/close | 0.41/1.84 | 0.03/0.07 ms | 0.06/0.26 ms | ≤ 1.8 | ≤ 1.8 |

First frame of a slide (includes the one-time layer flatten): dashboard close 7.8 ms, control
close 3.3 ms, launcher 1.6 ms; the old path paid the full repaint (up to 15.6 ms) on every frame.

Why the dropdowns were already fast: their surfaces are a fraction of the area with a few
primitives (calendar 3 in this fixture), so even a full repaint stays far under 8.3 ms.

Not explained by this bench: the live dashboard OPEN (27–29 ms gaps on the laptop). With the
fixture dashboard its open damage is only ~23% of the surface (the content does not slide;
only the body art grows) and the whole UI-thread step costs < 3 ms p95 before the fix, so the
live open gaps come from work this bench does not reproduce (live data/images, model updates
or non-render work on the UI thread during the slide). A live profile is needed (see Open).

## Checks

All on dev-2 in the ubuntu:24.04 CI container (debug unless noted):

- `cargo test -p bead-render slide_frames_composite_one_content_layer_matching_primitive_paint`:
  pass (open and close, 3 poses each: ≤ 2/255 channel difference from primitive paint, one
  layer build per slide, layer dropped when the slide ends).
- `cargo test -p bead-render`: 100 passed.
- `cargo test -p nacre-shell real_shell_panel_routes_open_and_close_with_prepared_scenes`: pass —
  12 real shell/ routes open and close, `panel.open` follows, and every reveal route's mid-slide
  frames (dashboard, launcher, control, calendar asserted; open and close) build exactly one
  content layer through `TreeCpuRenderer::render`.
- B0178: `cargo test -p bead-wayland companion_slide_callbacks_at_120_hz_keep_committing_without_staging`: pass.
- B0181: `cargo test -p bead-wayland close`: 5 passed.
- `cargo test --release -p nacre-shell frame_cost_bench -- --ignored --nocapture`: numbers above.
- Full CI per the B0156 recipe (ubuntu:24.04, Rust 1.99.0, Luau 0.742) on c0be6306's tree:
  `FULL_CI_OK` — fmt, clippy `-D warnings`, service boundary, workspace tests (1237 passed,
  0 failed, 9 ignored), trace memory-policy tests, no-default/all-features checks, facade and
  shell `--check`, Luau analyzer (175 modules), 12 spike suites, py_compile, packaging tests,
  wallpaper-video smoke, services check, private tray/clock bus tests, cargo-deny (all manifests).
- Not run: a fat-LTO release bench (the numbers use LTO off / 16 codegen units for both builds).

## Open

- Live dashboard open gaps are not reproduced by the bench (above). Proposed lead profile on the
  laptop: a `--profile profiling` build of this branch (frame pointers), `perf record -g -p
  <nacre-shell pid> -- sleep 12` while opening/closing the dashboard 3x, then
  `perf report --no-inline --children --comm nacre-shell --sort sym`.
- Sound (reveal=false, fade=true) already composites a flattened group per frame (the `Cached`
  opacity path), so its ~90 fps live is not paint cost either; likely the same non-render work.

- No temporary duplicates; `frame_cost_bench` is an ignored test (manual bench), not product code.
- The live laptop audit (PANEL_SLIDE frames and gaps per panel) is the acceptance; the bench is
  CPU work only and excludes wl_shm commit and compositor latency.
