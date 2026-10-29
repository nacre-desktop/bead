# B0202: panel slides smooth on battery

Goal: dashboard, launcher and control-centre slides at 120 Hz on the owner's laptop on battery
("balanced" profile), not only on AC. On battery every build ran these slides at ~60–80 fps
(live markers on 69da68b5: dashboard open 35–38 frames per ~400 ms with 46–52 ms gaps, close
13–22 frames with 39–63 ms gaps; launcher 27–48 ms; control centre 32–35 ms); the small status
dropdowns stayed at ~120 fps.

## Causes (live battery profile of the 69da68b5 profiling build)

1. `bead_render::tree::blend_over` was 26% of the UI (main) thread's samples (12% of the
   shell). It composites the B0185 slide content layer every frame, per pixel: a branch on alpha,
   a 4-byte copy for opaque pixels and four float multiply-adds for the rest. The layer is
   mostly opaque (cards, the closing panel) or empty (gaps between cards), so nearly all of that
   was a per-pixel copy that `memcpy` does several times faster.
2. Luau compilation (`luau_compile`, 6.5% of the shell, all on panel worker threads, none on
   the main thread). Panels are not retained when closed (`retain_when_closed: false`), so every
   open runs the panel factory: a new `Session`, hence a new generation. Its loader compiled each
   `.luau` file of the bundle twice (once in `Script::validate_source`, which also created a whole
   Lua VM per file, and again in `LuauFrontEnd::new`) and `behavior.luau` a third time in
   `Script::load`. B0191's persistent VM lives per generation, so it did not help here. On
   battery that compile work competes with the slide for the CPU's power budget.

Remaining main-thread work in the profile, not changed here (see Open): tiny-skia body paint of
the opening panels (~15%), per-frame contour SVG text and parse for the reveal artwork (~3%),
libc copies (~5%).

## Fix

- `bead-render` `blend_over` walks each row as runs: opaque runs are copied with
  `copy_from_slice` (memcpy), transparent runs skipped (both detected eight pixels at a time with
  a branch-free fold), and only partially covered pixels blended, in integer arithmetic:
  `s + (2·d·(255−a) + 255) / 510`. `d·(255−a)/255` is never exactly k + ½ (the numerator would
  have to be odd), so this equals tiny-skia's float `s + d·(1 − a/255)` rounded, bit for bit;
  a unit test checks all 254·256·256 (alpha, source, destination) channel values. The result is
  unchanged for every caller (the slide layer and raster items in `paint_items`); work is still
  limited to the damage ∩ reveal window as in B0185.
- `bead-ui` keeps one process-wide Luau bytecode cache (`frontend::bytecode`), keyed by two
  independent 64-bit SipHash digests of the source plus its length, with the default compiler
  options (identical to `Lua::load` of text). `LuauFrontEnd::new`, `Script::validate_source` and
  `Script::load` take bytecode from it, so a reopened panel compiles nothing and validation
  creates no VM. A compile error still goes through the old text path so diagnostics keep their
  chunk name and line. Budget: `markup-bytecode`, 4 MiB LRU (the source of all 175 shell modules is
  0.85 MB; bytecode is the same order); the least recently used entries are evicted at the
  budget. Edited source has a new hash, so hot
  reload compiles the edited files only.

Compositor-side motion (moving the layer surface's margin, or a subsurface with a
`wp_viewport` crop) was evaluated and not chosen: a reveal slide is not a pure translation of
the surface. The content moves behind a reveal window fixed to the bar/frame contour while the
body art grows (B0185), so moving the whole layer surface would move the frame join and the
body; a content subsurface would need a second buffer per panel, a synchronized subsurface
commit per frame, its own damage and input regions, and changes in the live surface paths
(B0175 open pose, B0181 close). With the CPU paste now a memcpy-class cost (table below), that
complexity is not justified; revisit if the live audit still shows render-bound gaps.

B0175 (open pose), B0178 (scheduler), B0181 (close motion) and B0185 (content layer split and
lifetime) are unchanged: only how the layer is pasted and where bytecode comes from changed.
No temporary duplicates. General engine primitives: run-based premultiplied source-over in the
CPU tree renderer; process-wide Luau bytecode cache in `bead-ui` (no Nacre ids).

## Numbers

Host: dev-1, ubuntu:24.04 container, release with B0185's bench overrides (LTO off,
16 codegen units), both builds back to back, each run pinned to ONE core (`taskset -c 5`) to
emulate a battery-limited budget; same core, same flags, two alternating rounds per build
(rounds agree within ~5%). `frame_cost_bench` (real `shell/` entries, scale 1.5, 5 slides per
row, owed regions = current + previous damage). ms per frame, UI-thread pose + prepare + damage +
paint, round 1 / round 2:

| Panel (surface px) | dir | total p50 before | after | change | paint p50 before | after |
|---|---|---|---|---|---|---|
| dashboard (1572x1071) | close | 0.999 / 0.940 | 0.487 / 0.447 | −51% | 0.987 / 0.925 | 0.475 / 0.434 |
| dashboard | open | 0.239 / 0.247 | 0.147 / 0.147 | −39% | 0.172 / 0.179 | 0.099 / 0.099 |
| launcher (1746x981) | close | 1.128 / 1.029 | 0.447 / 0.445 | −59% | 1.123 / 1.025 | 0.443 / 0.441 |
| launcher | open | 1.021 / 1.024 | 0.633 / 0.620 | −39% | 0.998 / 1.000 | 0.608 / 0.595 |
| control centre (732x1206) | close | 0.531 / 0.571 | 0.269 / 0.267 | −51% | 0.502 / 0.550 | 0.248 / 0.246 |
| control centre | open | 1.230 / 1.227 | 0.911 / 0.916 | −26% | 1.175 / 1.169 | 0.854 / 0.863 |
| calendar popout | close | 0.195 / 0.205 | 0.091 / 0.089 | −54% | | |

Closing slides (layer over the whole surface) halve. Opening slides keep the per-frame body
paint (tiny-skia fills of the growing body under the layer), which this unit does not change;
their p95 (dashboard 2.4 ms, control 2.9 ms) is that body paint and is the same in both builds.

Panel (re)open setup, all bench routes built again in one process (`B0202_SETUP`, the work a
reopen does on the panel worker): before 182–183 ms first / 148–149 ms again; after
156–157 ms first / 116 ms again (−22% on a reopen; the first build also benefits because routes
share modules and validation no longer creates a VM per file).

Memory: the bytecode cache holds the bytecode of every module loaded so far, process-wide, while
the shell runs (at most the 4 MiB budget; not yet measured live, expected well under 1 MiB for
the views the owner opens; the per-generation `compiled-markup` copy is unchanged). Budget for
the lead's audit: ≤ 1 MiB unreclaimable. This trades a small resident amount for no compile on each
open (smoothness first). The blend change allocates nothing.

## Checks

- `cargo test -p bead-render` (incl. new `integer_blend_matches_the_float_source_over_exactly`
  and B0185 `slide_frames_composite_one_content_layer_matching_primitive_paint`).
- `cargo test -p bead-ui`: 150 passed, incl. new `reloading_a_bundle_reuses_cached_bytecode`
  (first load compiles 3 sources, a second generation of the same bundle compiles 0).
- `frame_cost_bench` (release, numbers above).
- Full Bead CI (B0156 ubuntu:24.04 recipe, Rust 1.99.0, Luau 0.742) on this tree, including the
  real-shell route test `real_shell_panel_routes_open_and_close_with_prepared_scenes` (12 real
  `shell/` routes open and close through the content layer), B0178/B0181 Wayland tests.

## Open

- Acceptance is the lead's live laptop audit on battery (PANEL_SLIDE frames/gaps for dashboard,
  launcher, control centre). The bench is CPU only and excludes wl_shm commit and compositor.
- Next costs if opens stay short of 120 Hz on battery: the opening body paint (tiny-skia fill of
  the growing body each frame) and the reveal artwork's per-frame contour SVG (string format,
  parse, rasterize) — both visible in the live profile.
