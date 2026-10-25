# B0054 - Pearl memory and drag cadence

> B0128 retired the historical B0048 measurement script named below; current pearl-release/pacing harnesses remain. The measurements remain historical evidence.

Status: **Review** (Claude worker, branch `task/b0054-pearl-memory-20261023`). Follows B0048 (`0048-pearls.md`), whose memory gates failed.
Harness: `spikes/b0048-pearls.py` (`probe`, `memory`, `drag`, `smoke`); numbers are desktop nested niri in headless labwc, CPU renderer
(tiny-skia), no daemon, launcher from fixture, release builds with line tables, PSS in MiB.

## Where the memory went (probe: PSS split, glibc `mallinfo2`, cache budgets via `--trace-state`, smaps per phase)

Detached pearl vs the same panel docked, before: anon 14.29 -> 24.01 (+9.7). Three causes, all found with the cache-budget and heap numbers:

1. **Pearl art as two big rasters.** The old pearl drew the body as one 1100x590 SVG image (2.6 MiB) over the panel's shadow raster (3 MiB);
   the docked panel has one joined-frame raster (3 MiB). Now the body keeps the panel's root fill (pearl colour and corners) and the border is
   eight small pieces (four corners, four edges) drawn above the content (`pearls::border_svgs`, `Chrome::decorate_pearl`). Pixels: border and fill
   as before, now over the content as in QML. `retained-cpu-rasters` pearl 6.13 -> 3.59 MiB (docked 3.44).
2. **Stale rasters after the look change.** The old docked frame's raster stayed in the worker cache until the panel closed. The worker now drops
   entries no bake since the `PanelDetached` stage used, once that scene is accepted (`TreeCache::drop_unused_since`).
3. **Timers that only knew the resident panel.** The repair-scratch release (3.8 MiB, `cpu-repair-scratch`) and the idle SHM compaction/`malloc_trim`
   looked a surface up in the resident group, so a pearl's timers fired while its slot was not swapped in and dropped themselves. They now run
   through `on_surface`/`with_epoch`.

Closed settled +1.38 (file-backed): not code pages. smaps diff of control vs candidate after close: `MaterialSymbolsRounded.ttf` (896 KiB) and
`IBMPlexSans.ttf` (376 KiB) still mapped. `release_panel` idles the bar's worker (which drops the shared font system) only when no pearl is
swapped in, and closing a pearl never did it afterwards. `close_pearl` now idles the primary view when the last panel is gone.

## Before / after (3 alternating pairs, medians, CPU)

| Phase | before (B0048) | after | gate |
| --- | --- | --- | --- |
| pearl open - docked open | +11.97 (anon +10.5) | **+0.15** | <= +1: pass |
| closed settled, anon delta vs control | +0.02 | +0.04 | pass |
| closed settled, file-backed | +1.36 | 0 over its own idle (closed - idle: control 0.31, candidate 0.33) | pass, see note |
| idle / docked open / closed vs control | -0.01 / -0.05 / +1.38 | +0.67 / +0.66 / +0.68 | see note |

Note: this control is `a0c5d6f` (before B0047-B0050 landed); the candidate binary differs at idle by +0.67 already (code pages of a larger binary),
so the literal "+0.5 vs control" is not meaningful on this pair; the pearl-attributable part (closed minus idle) equals the control's. Needs a control
built from the commit before B0048 if the owner wants the literal number.

## Drag cadence

Shell-side cost per panel frame while pulling a pearl to the dock edge and back (180 pointer steps, guides on, `PANEL_FRAME processing_ms`): p50 0.15-0.17 ms,
p99 <= 4.8 ms (the one-off first frame; before this change 0.7). Guide scene prepare was already 0.3-0.5 ms (film SVG is small; the landing field is cached by hash).
Nothing per frame needed caching. Presented cadence in the nested stack is the stack's, identical for the old and the new binary:

| Outer compositor | presented p50 | p99 |
| --- | --- | --- |
| labwc pixman (B0048) | 40 ms | 61 ms |
| labwc gles2 (renderD129), this change | 32.5 ms (flat) | 32.6 ms |
| labwc gles2, B0048 binary | 32.2 ms | 32.4 ms |

**Target not demonstrated:** nested niri presents at a flat half rate here, so p99 <= 16.7 ms cannot be shown on this path; a lead capture on real output remains.
(`refresh_ns` is 0 in the nested feedback.) Rounds 2 and 3 of the drag harness lose the pearl (it docks); only round 1 is valid.

## Look-and-feel parity (owner rule 2026-10-23)

The border and fill are the same art as B0048, now cut into pieces and drawn over the content. Paired Bead before/after nested screenshots: 75 pixels differ by
more than 8 levels (corner anti-aliasing). **Not done:** side-by-side against the QML pearls (needs the Nacre QML shell in an identical nested session); border over content
matches the QML, border/film/guide timing and drag feel are unverified against it. Known gap until that capture is made.

## Generalization

Border, film and landing art stay generated SVG in `nacre-shell/src/pearls.rs`; not a kit/Luau component yet (ADR 0004 list updated). No temporary duplicates added.

## Validation

Goldens for pearls regenerated and eyeballed; `drop_unused_since`, the border pieces and the timer routing are covered by the pearl tests and the nested smoke (8 of 9 as in B0048,
`drag_moves_the_pearl` still the 57 px clamp). Full CI replica (Ubuntu 24.04 container on prod, the `check` job steps: fmt, clippy -D warnings, boundary, workspace tests 605 passed / 0 failed, features, facade, luau, spikes, packaging, wallpaper, services, deny) passed. Pearl goldens differ from B0048's by at most 4 anti-aliased corner pixels at 1.5.
