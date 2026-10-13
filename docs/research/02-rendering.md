# 02 — Rendering and text: costs to measure, not assume

Research date: 2026-10-13. Crate releases/licenses and source revisions/dates are
in [crate-versions.json](crate-versions.json) and [source-pins.json](source-pins.json).
The locked blank-surface spike is the only rendering implementation in Phase 0.
Text/images/effects are source research, **not rendered parity proofs** yet.

## Renderer comparison

| Candidate | Supplied capability / integration | Likely tradeoff and missing proof | License fit |
| --- | --- | --- | --- |
| wgpu 30.0.1 + Vello 0.11.0 | Compute-based paths, clips, gradients, glyph outlines/images; renders to an intermediate texture then presents | Attractive complex/vector scene path; device/pipeline initialization and cold shader compilation can dominate a shell's memory/startup. Multiple surfaces should share one device/renderer | MIT/Apache; eligible, but cold allocation result is a concern |
| femtovg 0.27.0 | Canvas-style vector paths, fills/strokes, images, clipping; OpenGL and optional wgpu backend; text integration paths | Smaller vector abstraction, straightforward Nacre primitives. Font fallback/color emoji/effects still need explicit integration; GL video interop may be simpler but is unmeasured | MIT/Apache; eligible. This spike measures wgpu/Vulkan, **not GL** |
| skia-safe 0.153.3 | Mature raster/GPU drawing, filters, text/image paths and native Skia facilities | Native build/artifact/dependency complexity, large capability surface; cold build/download cost and minimum GPU allocation must be measured before adoption | Wrapper MIT, Skia BSD; selected native/third-party build needs audit, including font libraries. Not automatically permissive because wrapper is MIT |
| tiny-skia 0.12.0 | CPU paths, strokes, gradients/patterns, clips and image blending | Very small idle baseline; no shaper/text editor, general filter/shadow engine or video decoder. Large animated surfaces can consume bandwidth/CPU | BSD-3-Clause; eligible first experiment |
| Slint software/Skia/femtovg renderers | Toolkit backends own caches, text/control drawing, dirty regions and platform integration | Valuable integration patterns and feature coverage; their overhead is not the standalone renderer's overhead. Internal code is not a permissive library we can extract | Slint runtime terms fail policy; use only as comparison |
| iced_wgpu 0.14.0 / iced_tiny_skia 0.14.1 | Primitive/image/SVG/text pipelines plus iced graphics/cache integration; selectable GPU/CPU | Mature UI-oriented rendering glue can reduce custom work, but generic runtime assumptions/layer surfaces/hot reload need adaptation | MIT; eligible to evaluate if direct integration/control work becomes too large |

Primary source reads: Vello `research/vello_research/src/{lib,util}.rs` and
simple example; femtovg `src/{lib,renderer/wgpu}.rs`, examples/helpers;
tiny-skia README/Cargo; Slint `internal/renderers/{software,skia,femtovg}`;
iced `renderer`, `wgpu` and `tiny_skia` manifests/source layout. The Vello
upstream tree now distinguishes experimental/research engines from CPU/GPU
projects; the **published vello 0.11.0** dependency and lockfile define the measured
engine, not a promise about every similarly named upstream package.

Reading aids: [Vello](https://github.com/linebender/vello),
[femtovg](https://github.com/femtovg/femtovg),
[rust-skia](https://github.com/rust-skia/rust-skia),
[tiny-skia](https://github.com/linebender/tiny-skia),
[Slint renderers](https://github.com/slint-ui/slint/tree/master/internal/renderers),
[iced renderers](https://github.com/iced-rs/iced/tree/master/renderer).
Use pinned commits from the ledger for implementation decisions.

## Text comparison

| Text stack | What it supplies | What Bead still needs |
| --- | --- | --- |
| cosmic-text 0.19.0 + swash | Multi-line shaping/layout, bidi, fallback and editing abstractions, raster glyph/color emoji cache; inspected main uses HarfRust | Glyph/image transfer into the chosen renderer, cursor/selection/focus, clipboard/IME, composition, accessibility and Nacre metrics |
| Parley 0.12.0 | Rich text layout/editing/selection utilities; Fontique discovery/fallback, HarfRust shaping, Skrifa font metrics/outlines, ICU4X analysis in inspected source | Raster/color-glyph integration for CPU renderer, UI editor semantics/IME and accessibility; actual glyph-format coverage must be tested |

Read [cosmic-text source/README](https://github.com/pop-os/cosmic-text) and
[Parley text stack](https://github.com/linebender/parley/blob/main/README.md).
Both are MIT/Apache; Parley's data/test/font assets have their own notices.
“Supports emoji” does not establish all COLR/CPAL, CBDT/CBLC and SVG emoji formats
on the selected backend. Test the installed color font, ZWJ family emoji,
modifiers/flags, mixed Arabic/Latin, CJK fallback, combining marks and bidi
selection. A missing font, tofu glyph or broken cursor is a failed parity test.

Provisional choice: cosmic-text for the first CPU-based rendering/text/input
proof, because its swash raster cache matches tiny-skia integration and includes
editing machinery. Parley is a strong alternative if rich text and Vello become
the better measured path. Do not load both font stacks permanently without a
demonstrated need. Use a single shared font system/cache with bounded glyph and
size/scale generations; system font discovery can be a meaningful cold-start
cost. Neither text choice's Nacre memory or latency is measured in Phase 0.

## Images, SVG, icons and effects

- `image` 0.25.10 with selected PNG/JPEG/GIF/WebP codecs decodes off the UI thread.
  Bound dimensions/decompressed bytes, preserve image aspect/cropping and key
  caches by content/size/scale. AnimatedImage needs frame disposal/timing; it is
  not an ordinary repeated PNG load. Enable only audited required codecs.
- `usvg`/`resvg` 0.48.1 (MIT/Apache) parse/rasterize SVG using tiny-skia; preserve
  scaling/theme color where the icon workflow requires it. Raster SVG at target
  scale and cache, rather than reparse every frame. Font/text in SVG and embedded
  images need separate cache/limit policy.
- `freedesktop-icons` 0.4.0 (MIT) is an eligible lookup helper. Check actual theme
  inheritance, hicolor fallback, index.theme, symbolic/name variants and requested
  physical size against the
  [Icon Theme 0.13 specification](https://specifications.freedesktop.org/icon-theme-spec/0.13/).
  Lookup libraries do not license the icon assets themselves; retain each asset's
  notice and use the installed Nacre theme for comparison.
- Rounded rects, borders and clips use renderer primitives. Match the current
  Nacre contour/SVG details instead of designing new shapes. CPU shadow/effect
  support is a potential gap: tiny-skia is not a general blur engine. Prefer
  existing permissive effect functionality or bounded cached shape/shadow assets;
  do not quietly write a new renderer/filter framework.
- Local content effects, client-drawn shadows, compositor background blur and
  compositor window shadows are different mechanisms. MultiEffect/Canvas parity
  may justify a GPU path later. Report unsupported effects explicitly until a
  matched primitive/effect scene demonstrates them.

## Measurement protocol

The spike uses the same SCTK connection/layer shell/event dispatch for all Rust
renderers, `512×32` logical pixels, scale 1, one output, neutral `#202020` fill,
zero exclusive zone/no keyboard focus. It renders once and then blocks for
events. The GPU modes share Vello's utility context/surface setup so they use
the same Vulkan device/presentation path; femtovg is therefore a **wgpu backend
integration baseline**, not a minimal standalone OpenGL context. Its otherwise
unused small intermediate target/blitter is a disclosed harness overhead.

Quickshell 0.3.2 / Qt 6.12.0 runs only spikes/blank.qml with the same dimensions,
color, anchor/focus/zone policy. No Nacre services, wallpapers, controls or fonts
are loaded. It is not the full 212/306 MiB shell reference. Each candidate receives
three complete runs with 5 s settling and 60 s idle sampling; use all cold/warm
rows rather than cherry-picking the minimum.

Isolation: nested niri 26.04 on an offscreen labwc 0.20.2/GLES parent, Intel ARL
integrated GPU, Mesa 26.2.4. Private D-Bus with XDG isolation **before bus startup**
also isolates activated portal helpers. Separate config/cache/state/data/runtime;
explicit system data dirs, no session launch or live-shell command. The final
campaign records registry/output properties, selected GPU adapter, raw smaps,
Qt scene-graph logs, screenshots and DRM fdinfo where available. Client totals
exclude helper/compositor processes. The nested environment includes niri's
startup hotkey overlay; it is the same static compositor UI across clients.

`/proc/PID/smaps_rollup`: PSS apportions shared resident pages; USS sums private
clean/dirty/hugetlb pages. SwapPss is recorded separately. PSS does **not** include
all GPU allocations. DRM fdinfo is driver accounting, may include duplicate
file descriptions and shared objects, and must not simply be added to PSS.
CPU is delta user+system ticks divided by wall time and CLK_TCK, reported as a
percentage of one logical CPU. Zero ticks over 60 s means below sampling
resolution, not a proof of zero electrical power or compositor GPU activity.

Rust READY is the first wl_surface frame callback observed by the client; the
QML marker is component completion and **not comparable first-frame startup**.
Both are recorded honestly, but no startup speedup is calculated. Use common
presentation feedback/timestamps at Feature 1/2 for the required fair startup
comparison. Hot-reload latency and frame pacing are **not measured** by a blank
static surface; neither has been implemented in Phase 0.

Final numeric table is generated in [baseline-summary.md](baseline-summary.md)
from [baseline.json](../../spikes/results/baseline.json). Raw local logs and
screenshots are ignored; a sanitized nested screenshot is retained with the
report. Reproduce with `./spikes/run.sh` from a Bead checkout once the listed
system prerequisites/isolated tools are available.

## Observed issues and interpretation

- Initial visible nested runs were interrupted when niri exited cleanly; one
  client reported broken pipe. Root cause of the external closure was not
  established. These incomplete/5-second samples are excluded.
- One interrupted campaign suggested Vello 187→4 MiB PSS. It cannot establish
  successful trimming. The stable repeated campaign showed a large **first-run
  cost** and lower later runs, with no swap recorded. Shader/cache state is a
  hypothesis for that difference; allocator attribution remains unverified.
- Initial QML READY logs were filtered at the default logging level. The client
  itself loaded; absent markers are failures of the harness, not proof that QML
  cannot run. Explicit qml.debug logging fixed the marker.
- The first private bus was started before XDG isolation for activated services.
  Corrected final campaign isolates it before launch; only that final campaign
  supplies the final headline numbers.

## Capacity bounds and next tests

A 512×32×4-byte CPU buffer is 64 KiB. A 1920×1080 RGBA buffer is 7.91 MiB,
and 3840×2160 is 31.64 MiB; double buffering doubles those figures before caches.
At 4K/60 Hz, one full RGBA buffer copy is about 1.99 GB/s. These are arithmetic
bounds, **not measured traffic**, and show why a tiny bar says little about full
output-sized animated drawers/wallpaper. Keep damage and target sizing explicit.

Recommendation: start with tiny-skia + cosmic-text + Taffy for a minimal surface
and the reload proof. Keep an interface capable of a lazy shared GPU renderer;
femtovg/wgpu is the first GPU alternative from current measurements. Do not load
both eagerly. Vello remains a measured alternative, with its first-run allocation
treated as a concern. Before selecting the full-shell default, test an actual
Nacre-sized rounded/shadowed/text/image scene and animated panel on one/two
outputs. Skia/GL measurements are **not done**, and no numbers are inferred.

**Verdict: concern / proceed only through early gates.** Low blank CPU memory
is promising; full visual/input parity and animated bandwidth are still open.
