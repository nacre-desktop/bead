# Benchmark pass: frame gaps and allocations per frame (2026-11-14)

Measurement only, at `841de925`: every runnable example, scripted, at 120 Hz. No product code changed.

## Method

- **Rig.** Release `bead run <example>` (minimal-shell: its own host binary) in a private headless sway 1.9 with one 1920x1080@120 Hz output, pixman compositor, Bead CPU renderer. `bead` builds without a GPU renderer, so the GL/wgpu path was not measured. The prebuilt CI image ran in a 6-CPU container on a shared build host. Desktop services the container lacks (audio, battery, media, power) show as unavailable, so their buttons do nothing.
- **Script** (private virtual pointer, `wtype`). Hover sweeps; presses across every control; wheel steps; typing where there is an input. In bar-with-panels, open, hover and close each panel twice, and press More/Less on the status peek (see the caveats). In graph-demo, hover, select, zoom and pan. Then 10 s with no input, then a scale change as a positive control: an arm with no traced frames there fails its self-check, as does a missing phase, frame timestamp or allocation field, or any `UI_ERROR`.
- **Builds, 2 runs each.** *Base* (the tree as is) gives timing and CPU. *Count* adds a bench-only patch, not committed: a counting global allocator and allocation fields on the frame trace. It gives allocations. One count run also sets `BEAD_PROFILE_INPUT` for stage spans.
- **Metrics.** Commit timestamps come from `BEAD_FRAME_TRACE`: `UI_FRAME` for layer surfaces, `WINDOW_FRAME`/`WINDOW_PRESENTED` for windows. Gaps under 50 ms count as one burst of frames; a longer gap is the pause between scripted inputs. A burst gap over 12 ms is a missed 120 Hz vblank. It means jank only in motion (animations, drags); during hover, presses and typing it follows the input cadence. CPU per frame is process CPU (schedstat, all threads) in the phase divided by its frames. Damage and the prepare/paint split come from `UI_FRAME`. Allocations per frame are counted on the render thread and across the process. "Empty" frames carry no damage.

## Results (timing from base, allocations from count; 2 runs pooled)

| example | phase | frames (empty) | burst gap p50/p95/max ms | >12 ms | CPU ms/frame | allocs/frame thread/process | KiB/frame | damage kpx/frame | prepare/paint ms |
|---|---|---|---|---|---|---|---|---|---|
| graph-demo | pan (drag) | 81 (0) | 16.7/18.5/20.6 | 79/79 | 16.71 | 18/170 | 1948 | 1942 | window path |
| graph-demo | zoom (wheel) | 40 (0) | one frame per step | – | 16.45 | 18/237 | 2447 | 1942 | window path |
| graph-demo | hover | 149 (0) | 14.0/32.3/50.0 | 91/120 | 13.78 | 24/658 | 2125 | 1063 | window path |
| kit-gallery | press | 48 (0) | 8.4/32.7/33.1 | 10/22 | 9.07 | 88/42895 | 8022 | 25.5 | window path |
| kit-gallery | hover | 78 (0) | 15.3/42.3/42.3 | 22/40 | 1.92 | 53/4780 | 805 | 15.1 | window path |
| bar-with-panels | status open (motion) | 137 (12) | 8.1/10.6/30.9 | 4/125 | 0.87 | 125/560 | 833 | 39.4 | 0.49/0.14 |
| bar-with-panels | status close (motion) | 93 (20) | 8.1/31.8/32.6 | 4/77 | 0.60 | 103/200 | 423 | 38.6 | 0.31/0.14 |
| bar-with-panels | calendar / volume open | 40 / 28 (24 / 20) | 9.4/30.8/31.1 | 4/24 | 0.51 / 0.71 | 34/1448 | 721 | 10.5 / 15.1 | 0.00/0.09 |
| bar-with-panels | hover (bar / panel) | 32 / 32 (28 / 20) | input cadence | – | 1.85 / 2.32 | 29/1005 | 94 | 8.6 | 0.00/0.08 |
| hello-status | press | 194 (192) | 0.1/0.2/0.3 | 0/96 | 0.46 | 22/– | – | 1.5 | 0.00/0.03 |
| tutorial-bar | press | 66 (64) | 0.1/30.4/30.6 | 2/32 | 0.45 | 22/512 | 48 | 2.1 | 0.00/0.03 |
| hello-bar | press | 14 (12) | 0.1/0.2/0.2 | 0/6 | 1.43 | 26/– | – | 9.9 | 0.00/0.07 |
| hello-window / minimal-shell | type | 38 / 38 (0) | input cadence | – | 2.11 / 2.37 | 8/784 · 21/925 | 324 / 642 | 0.2 / 5.5 | window path |
| hello-window / minimal-shell | press | 20 / 20 (0) | input cadence | – | 0.80 / 0.93 | 11/916 · 10/1003 | 503 / 529 | 0.1 | window path |
| all examples | idle 10 s | 0 | – | – | 0.00-0.04 % CPU, 1-6 wakeups/s | – | – | – | – |
| all examples | hover, no visual change | 0 | – | – | 0.5-1.2 % CPU, 265-350 wakeups/s | – | – | – | – |

The status peek's motion frames hold 120 Hz: motion-to-motion gaps are p95 8.3 ms, max 9.1 ms on open and p95 9.4 ms, max 9.9 ms on close. The ~31 ms gaps in open/close phases fall between the press frame and the release frame of one click, so they are not dropped frames. In the traced calendar open, the panel's first content frame lands about 3 ms after the release.

## Top 5 offenders, with attribution and a proposed follow-up each

1. **graph-demo pan/zoom: 16.5 ms of CPU raster per frame, so 60 Hz.** All 79 pan gaps exceed 12 ms (p50 16.7 ms) at 52 % CPU, and every frame damages the whole 1.94 Mpx canvas. The spans per frame are `paint_graph` 10.6 ms and `paint_rounded` 6.4 ms, on the main thread (`MAIN_BUSY ui:Window`). Attribution: prepare/raster plus full-surface damage. *Follow-up:* keep the painted graph as a retained raster layer in `bead-render`. During a pan, blit it at the new offset and repaint only the exposed strip. During a zoom gesture, scale the cached layer and re-raster once the gesture rests.
2. **graph-demo hover: 13.8 ms per frame for a highlight change.** Each hover frame damages 1.06 Mpx, 91 of 120 burst gaps exceed 12 ms, and spans per frame are `paint_graph` ~10 ms plus `paint_rounded` ~3.9 ms. Attribution: damage scope plus raster. *Follow-up:* in `GraphCanvas`, damage only the old and new highlighted nodes and their edges, and paint the highlight as an overlay above the cached layer from item 1.
3. **kit-gallery press: 9.1 ms CPU, 42.9k allocations and 7.8 MiB per press frame.** Only 0.24 ms and 88 allocations of that are on the render thread (count build). Every press and every release re-runs the whole Luau view: 78 `markup_eval` for 38 clicks. Each run costs about 0.57 ms of eval, 0.61 ms of `markup_serde`, 0.47 ms of `view_ir`, 0.20 ms of layout and 0.14 ms of bake. Attribution: Luau plus scene build on the worker. *Follow-up:* find which store the kit components write on press/release, and keep pressed/hover visuals as engine-side interaction state, so a press re-evaluates nothing, or at most the pressed component. Hover is a separate cost and does not re-run the view: 4.8k allocations, 805 KiB and about 1.4 ms of worker time per frame. Profile it after this fix.
4. **bar-with-panels status peek: prepare re-runs on every motion frame.** It holds 120 Hz, but pose and prepare take 0.49 of 0.66 ms render-thread time per frame. There are 125 render-thread allocations per frame and 833 KiB per frame process-wide. Nearly all of the 0.49 ms `present_prepare` per frame is the reveal art re-raster with its shadow (`reveal_art_raster` and `shadow_raster`, nested spans, about 0.5 ms each). `shadow_pixmap` (`bead-render/src/lib.rs`) caches only two fixed shapes, so every other shadow, including each new body size during the reveal, re-blurs an SVG. Attribution: prepare/raster. *Follow-up:* draw box shadows from a nine-slice raster cached per (radius, blur, scale), and remove the fixed-shape cache. Reuse the prepared reveal art across pose-only frames; B0272 proposed the same fix.
5. **Pixel-identical generations commit frames that carry no damage.** On layer bars, nearly every input that changes no pixel still commits: 192 of 194 hello-status press frames, 64 of 66 tutorial-bar, 28 of 32 bar-with-panels hover frames. About half are a second commit 0.1-0.3 ms after an immediate frame callback (96 of 194 on hello-status). Each costs only 0.03-0.1 ms of render time, but it is a worker generation, a commit and a compositor wakeup. Attribution: upload/commit and idle wakeups. The callback-only commit for an unpresented generation is in `rendering.rs`, the `regions.is_empty()` branch. *Follow-up:* have the worker drop a generation whose baked scene equals the presented one, so input without a visual change neither commits nor re-arms a callback.

## Not measured, and caveats

- The GL/wgpu renderer: `bead-cli` turns off `bead`'s default `native-gl` feature, and a GPU build plus lavapipe was not cheap here. Real-hardware presentation timing: headless sway sends frame callbacks for buffer-less commits at once, which inflates item 5's duplicate count. Live desktop numbers decide performance.
- Status grow/shrink: More/Less were pressed with the keyboard, which did not reach the panel in this session (0 frames). A pointer-driven rerun was queued but got no build slot in time. Process-wide allocations are missing for phases without frames (idle, hover with no change) and for bar press phases, because the 10 Hz totals stream was in that rerun too.
- Panel services (audio sliders, mute, media, power) were inert without the services. Typing invokes `wtype` once per key for BackSpace, so each of those keystrokes also carries a keymap upload. Threads are unnamed, so off-render-thread CPU is split only by profile spans.
- The `BEAD_PROFILE_INPUT` run adds per-frame logging; its spans are for attribution only and are not in the table.
