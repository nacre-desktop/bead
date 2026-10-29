# B0205: battery main-thread cost of panel slides

Goal: after B0202, panel slides on battery still ran at ~60–80 fps with 30–60 ms stalls
(c4c9c943 audit: dashboard open gaps 38–67 ms, close 45–57 ms, launcher 31–40 ms, control
centre 31–48 ms). The main thread used ~160 ms of CPU per dashboard open + close; B0202's bench
put a slide frame at ~1 ms, so the cost had to be found first.

## Markers (phase 1, `BEAD_FRAME_TRACE=1`, nothing read or allocated when off)

- `MAIN_BUSY`: each event-loop pass with ≥ `BEAD_FRAME_TRACE_BUSY_MS` (default 4) of main-thread
  CPU or attributed wall, with exclusive `stage:wall/cpu xN` for `ui:<event>`,
  `panel_ui:<event>`, `draw`/`draw_panel`, `decorations`, `start_panel`, `close_panel`,
  `release_panel`; `other_cpu_ms` is Wayland dispatch and unnamed work.
- `UI_FRAME` (prefix unchanged) adds `wall_ms cpu_ms gap_ms serial damage_px motion` and a wall
  split `decorations,pose,prepare,damage,[gpu_fallback,]paint,commit`.
- `PANEL_TRACE`: `start_panel`, `worker_session`, `worker_scene`, `panel_content_ready`,
  `close_panel`, `release_panel` with wall/CPU.

General engine primitives: `bead_core::profile::{Busy, busy_iteration, split, take_splits,
trace, variant_name}`.

## Causes (live laptop capture of the c4c9c943 + markers profiling build)

The capture ran with the profiling binary and per-frame journald logging, partly on AC, so the
absolute numbers are inflated; relative to dev-2 one-core numbers every main-thread cost was
~15× larger (a ~2 ms dev-2 decoration set cost 30–50 ms; a ~7 ms dashboard flatten cost
100–137 ms), which matches a down-clocked core rather than one hot spot. What blocked:

1. **Close slide flatten.** B0185 flattens the slide content on the slide's first frame on the
   UI thread. Closing, that is the whole panel: the first close frame took 117–137 ms (dashboard,
   1.6 Mpx) and 60–63 ms (launcher), the single largest gap of every close.
2. **Worker join on release.** `worker::Source::drop` joined the panel worker thread on the UI
   thread; a worker only sees `Stop` between jobs, so replacing a panel mid-stage blocked the
   main thread for 537 ms once (9 ms CPU).
3. **Decoration tile thrash.** Control centre and status popout opens rasterised two tile sets
   (the placeholder's shape, then the content's) and their closes another one, 25–52 ms each:
   with the placeholder sets the four-set cache never held the idle frame plus two panels.
4. Not changed here (see Open): mid-open re-flattens when a new worker scene or new rasters
   arrive during the slide (52–120 ms frames with a `serial` change or a 13–17 ms `prepare`),
   and the first open flatten (~20 ms live).

## Fix

- `bead-render`: a reveal panel resting open schedules its close layer as quiet-time work
  (`Renderer::idle_work`, run from the existing transient-resource timer once the content has
  been unchanged for 150 ms). A `bead-close-layer` thread flattens the whole scene (same items,
  rasters and band painter as the synchronous path, `flatten_layer`); the close slide adopts it
  when its content key matches (joining a job that is still running, never painting twice) and
  otherwise flattens as before. Any later content change starts one new job (superseded jobs
  finish unseen). The first close frame no longer paints on the UI thread.
- `bead-wayland` worker: dropping a `Source` sends `Stop` and hands the join to a `bead-reap`
  thread, which also returns the freed heap (`malloc_trim`) after the session is gone; the UI
  thread never waits for a busy worker.
- `bead-wayland` decorations: while the panel shows nothing yet (not ready, no ready fallback)
  the frame keeps its current tiles instead of rasterising a join for the placeholder's shape;
  the cache keeps six tile sets (was four).

B0175 (open pose), B0178 (scheduler), B0181 (close motion), B0185 (content layer, its pixels and
lifetime during a slide) and B0202 (run paste, bytecode cache) are unchanged; the adopted layer
is bit-identical to the synchronously flattened one (unit test).

## Memory

- Close layer: one premultiplied layer of the panel bounds while a reveal panel rests open
  (dashboard 1572×1071 = 6.7 MB, launcher 6.9 MB, control centre 3.5 MB), charged to the existing
  64 MiB `cpu-composite` budget; it becomes the close slide's layer and is dropped after the
  slide, so nothing is held while closed. Budget: ≤ 7 MiB anon while a large panel is open,
  0 when closed (smoothness first; the slide held the same layer before, only later).
- Decoration tiles: two more cached sets, ≤ 0.6 MiB each at 1920×1200 @ 1.5 (≤ +1.2 MiB worst
  case, bounded by the 2 MiB per-set rule).
- Reaper threads: one short-lived thread per released panel worker.

No temporary duplicates.

## Numbers

`frame_cost_bench` (real `shell/` entries, scale 1.5, dev-2 Arch container, release with LTO off
and 16 codegen units, pinned to one core with `taskset -c 5`). Close slides start from the panel
resting open; "before" sets `B0205_NO_IDLE=1` (no quiet time before the close, which is the old
behaviour), "after" lets the 150 ms quiet time pass. Worst close frame (UI thread, ms):

| Panel | before | after |
|---|---|---|
| dashboard | 6.69 | 0.67 |
| launcher | 2.32 | 0.65 |
| control centre | 3.86 | 0.38 |
| calendar | 0.17 | 0.15 |

Close p95 is unchanged (0.56–0.61 ms dashboard/launcher). Live, the removed frame was the 117–137 ms
dashboard and 60–63 ms launcher close frame of the capture.

## Checks

- `cargo test -p bead-render`: new `a_resting_panel_flattens_its_close_layer_off_the_ui_thread`
  (no flatten before 150 ms quiet, one background flatten per content key, adopted pixels equal a
  fresh renderer's for three close poses, zero UI-thread flattens) and the B0185 slide tests.
- `bead-wayland` worker shutdown test now stops and joins explicitly.
- Full Bead CI (ubuntu:24.04, ci.yml steps) including the real-shell route test.

## Open

- Acceptance is the lead's live battery audit (`PANEL_SLIDE` plus `UI_FRAME`/`MAIN_BUSY` with
  `BEAD_FRAME_TRACE=1`).
- Opens: re-flatten when a new worker scene (or new rasters) lands mid-slide, and the ~20 ms
  first open flatten. Next steps: repaint only the changed items into the open layer, or let the
  first open frame wait for a background flatten like the close.
- Retaining several closed panels (control/popouts are released whenever another panel opens)
  was not done; with the join off the UI thread, the remaining cost of a release is ~9 ms CPU.
