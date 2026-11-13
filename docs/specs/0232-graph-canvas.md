# 0232 — GraphCanvas feasibility

Status: phase 2a engine, CPU path and GPU hookup implemented; memory gate missed by 0.35 MiB (1.35 MiB median against 1 MiB, floor per component below), lead decision pending; the Luau contract stays provisional until the consumer's design arrives; phase 2b adds the retained GPU path and validates 120 Hz on real GPUs; phase 2c adds activation, highlight, keyboard focus and GPU labels above the graph for the reader-beside-graph view.

Target: 2,000 nodes / 10,000 edges at 120 Hz (8.33 ms/frame), including pan/zoom; quiescent when unchanged.
Applications own content, persistence and theme policy; phase 2b API refinements await consumer design.

## Evidence and limitations

Release, offscreen measurements use dev-1 (Ryzen 9 9950X, 32 host threads),
Ubuntu 24.04, Rust 1.99.0/workspace release profiles, fresh 6-CPU / 12-GiB slot; target inside.
CPU and llvmpipe results use seven samples/case (nearest-rank p95 is max at n=7); real-GPU
results use five samples/case (p95 is max at n=5). Times are milliseconds.
The same seeded 2k/10k graph spans 1480×1000 world units; viewport 1280×800,
scale 1 / 2.25; pan (4,2), zoom ×1.01, one visible hover target.
RGBA output; no Wayland presentation/readback/compositor scheduling is timed.
Default CPU includes Scene build, tree prepare, semantic damage and draw; retained CPU
uses a synthetic one-record state comparison and vector painting. GPU includes scene
assembly/cache preparation and completed submission waits; geometry construction is separate.

Each frame cell is **median/p95 ms**; “full” is a warm full repaint, not first paint.

| Host | Renderer / path | Resolution | Full | Pan | Zoom | Hover |
|---|---|---|---:|---:|---:|---:|
| dev-1 | CPU ordinary | 1280x800 | 6466.72/6552.10 | 6401.93/6462.47 | 6816.73/6952.87 | 333.46/335.97 |
| dev-1 | CPU ordinary + LOD | 1280x800 | 1630.53/1652.52 | 1609.48/1691.27 | 1677.39/1711.89 | 83.64/86.31 |
| dev-1 | CPU legacy + tree metadata | 2880x1800 | 1733.16/1819.48 | 1656.08/1741.27 | 1726.39/1880.64 | 311.45/316.71 |
| dev-1 | CPU legacy + LOD | 2880x1800 | 465.47/472.17 | 465.36/482.87 | 465.90/475.25 | 79.76/81.04 |
| dev-1 | CPU retained vector | 1280x800 | 212.77/220.00 | 213.09/213.86 | 486.04/501.83 | 213.53/220.11 |
| dev-1 | CPU retained vector | 2880x1800 | 798.50/800.21 | 805.95/812.55 | 803.58/806.51 | 801.50/805.31 |
| dev-1 | CPU retained vector + LOD | 1280x800 | 68.66/71.99 | 56.79/67.37 | 94.59/107.81 | 55.71/56.22 |
| dev-1 | CPU retained vector + LOD | 2880x1800 | 185.76/186.71 | 186.30/187.35 | 186.40/188.64 | 186.97/187.61 |
| dev-1 | CPU vector + LOD, zoom 0.4 | 2880x1800 | 52.95/54.56 | 52.06/52.13 | 53.67/55.44 | 52.47/54.09 |
| dev-1 | Vello raster, llvmpipe | 1280x800 | 3.37/3.85 | 3.37/4.04 | 618.84/626.04 | 361.66/374.32 |
| dev-1 | Vello raster, llvmpipe | 2880x1800 | 8.66/8.79 | 8.48/9.26 | 1951.69/2016.60 | 1941.69/1996.99 |
| dev-1 | Vello raster + LOD, llvmpipe | 1280x800 | 2.66/3.94 | 2.80/3.07 | 160.50/165.44 | 94.36/99.83 |
| dev-1 | Vello raster + LOD, llvmpipe | 2880x1800 | 7.99/8.31 | 8.11/8.56 | 502.89/520.15 | 495.19/509.69 |
| dev-1 | Vello vectors, llvmpipe | 1280x800 | 3.27/4.10 | 3.09/66.75 | 3.37/60.31 | 3.25/3.29 |
| dev-1 | Vello vectors, llvmpipe | 2880x1800 | 6.73/48.72 | 5.92/76.36 | 6.32/75.10 | 6.86/75.74 |
| dev-1 | Vello vectors + LOD, llvmpipe | 1280x800 | 15.56/82.31 | 15.83/83.55 | 14.19/83.77 | 14.52/83.60 |
| dev-1 | Vello vectors + LOD, llvmpipe | 2880x1800 | 3.87/4.01 | 3.68/4.46 | 3.60/4.17 | 3.85/42.89 |
| desktop NVIDIA GeForce RTX 4070 / driver 615.78.08 | Vello raster | 1280x800 | 1.227/1.265 | 1.481/2.062 | 641.993/649.233 | 379.030/379.594 |
| desktop NVIDIA GeForce RTX 4070 / driver 615.78.08 | Vello raster | 2880x1800 | 5.977/6.014 | 6.130/6.385 | 1747.802/1882.373 | 1741.760/1747.935 |
| desktop NVIDIA GeForce RTX 4070 / driver 615.78.08 | Vello raster + LOD | 1280x800 | 2.304/2.348 | 2.257/2.613 | 166.958/168.372 | 100.305/101.770 |
| desktop NVIDIA GeForce RTX 4070 / driver 615.78.08 | Vello raster + LOD | 2880x1800 | 5.826/6.155 | 5.999/6.130 | 455.697/457.716 | 456.763/492.771 |
| desktop NVIDIA GeForce RTX 4070 / driver 615.78.08 | Vello vectors | 1280x800 | 2.584/2.682 | 2.736/2.849 | 2.721/2.789 | 2.664/3.012 |
| desktop NVIDIA GeForce RTX 4070 / driver 615.78.08 | Vello vectors | 2880x1800 | 5.040/5.069 | 5.482/5.644 | 5.149/5.268 | 5.109/5.424 |
| desktop NVIDIA GeForce RTX 4070 / driver 615.78.08 | Vello vectors + LOD | 1280x800 | 12.095/12.468 | 12.644/12.697 | 12.672/13.013 | 1.522/3.255 |
| desktop NVIDIA GeForce RTX 4070 / driver 615.78.08 | Vello vectors + LOD | 2880x1800 | 3.847/3.998 | 4.016/4.461 | 4.150/4.215 | 4.045/4.472 |
| desktop AMD Ryzen 5 7600 6-Core (RADV RAPHAEL_MENDOCINO) / Mesa 26.2.5-arch3.1 | Vello raster | 1280x800 | 1.851/2.949 | 1.711/2.709 | 642.029/643.223 | 377.502/378.060 |
| desktop AMD Ryzen 5 7600 6-Core (RADV RAPHAEL_MENDOCINO) / Mesa 26.2.5-arch3.1 | Vello raster | 2880x1800 | 5.099/5.113 | 5.133/5.257 | 1734.236/1766.807 | 1738.417/1754.230 |
| desktop AMD Ryzen 5 7600 6-Core (RADV RAPHAEL_MENDOCINO) / Mesa 26.2.5-arch3.1 | Vello raster + LOD | 1280x800 | 1.337/1.362 | 1.334/1.387 | 166.968/170.863 | 99.542/101.198 |
| desktop AMD Ryzen 5 7600 6-Core (RADV RAPHAEL_MENDOCINO) / Mesa 26.2.5-arch3.1 | Vello raster + LOD | 2880x1800 | 4.431/4.459 | 4.539/4.546 | 447.530/451.262 | 447.221/448.430 |
| desktop AMD Ryzen 5 7600 6-Core (RADV RAPHAEL_MENDOCINO) / Mesa 26.2.5-arch3.1 | Vello vectors | 1280x800 | 2.381/2.421 | 2.390/2.467 | 2.345/2.421 | 2.348/2.434 |
| desktop AMD Ryzen 5 7600 6-Core (RADV RAPHAEL_MENDOCINO) / Mesa 26.2.5-arch3.1 | Vello vectors | 2880x1800 | 10.424/10.579 | 10.816/11.538 | 10.947/11.540 | 10.435/10.631 |
| desktop AMD Ryzen 5 7600 6-Core (RADV RAPHAEL_MENDOCINO) / Mesa 26.2.5-arch3.1 | Vello vectors + LOD | 1280x800 | 20.770/21.258 | 20.683/20.799 | 20.805/20.912 | 20.676/20.839 |
| desktop AMD Ryzen 5 7600 6-Core (RADV RAPHAEL_MENDOCINO) / Mesa 26.2.5-arch3.1 | Vello vectors + LOD | 2880x1800 | 1.668/2.353 | 1.534/2.163 | 2.402/2.554 | 2.148/2.542 |

The 852-byte loop-generated Luau view failed 7/7 attempts at the 8 MiB VM limit:
**12.954/14.747 ms** to rejection, not a frame. A literal fixture also hits the 1 MiB
source cap; the 8,192-view-node cap independently excludes 12k widgets. Limits were unchanged.
Direct Scene rows exclude Luau/layout; they do not represent successful 12k-widget views.
Default CPU at 2880×1800 failed 7/7 full draws: 16 MiB repair-scratch guard,
**112.378/119.570 ms to error**; no valid pan/zoom/hover baseline. Legacy CPU rows
retain actual tree prepare/damage but use the existing legacy painter to bypass that guard.

LOD means conservative endpoint-AABB culling, every fourth edge, alpha 85 versus 150;
ordinary/legacy rows include 16 priority labels. Separate 2880×1800 probes (median/p95 ms):

| Legacy + tree metadata case | Median | p95 |
|---|---:|---:|
| Culling only, 16 labels | 1639.276 | 1669.083 |
| Quarter edges + alpha only, 16 labels | 411.441 | 420.750 |
| No labels, all edges | 1633.539 | 1646.144 |
| 2,000 labels, admission rejected 7/7 (1,024 retained-entry guard) | 29.936 | 30.773 |

Scene construction: 0.206/0.229 ms; retained CPU path build: 0.279/0.304 ms
(full) / 0.182/0.213 ms (LOD). Retained CPU merges edge intersection alpha, omits
labels and repaints fully on hover: an optimistic proxy, not pixel-equivalent integration.
GPU vectors preserve edge compositing, omit labels and append encoded geometry per frame;
these are synchronized offscreen latencies, not presentation FPS. Each current-Vello
case sample uses a fresh renderer warmed outside timing; earlier persistent repeated zooms
hit its raster byte-budget guard at 2880×1800. First paint/initial ingestion are not certified.
The original GPU rows above are **llvmpipe, not representative** (LLVM 20.1.2/Mesa 25.2.8).
Real-GPU rows are fresh-renderer, synchronized offscreen timings on the desktop test host;
RTX 4070 used NVIDIA 615.78.08, and integrated graphics used AMD Ryzen 5 7600
(RADV RAPHAEL_MENDOCINO, Mesa 26.2.5-arch3.1). Both Vulkan adapters were exposed.
All 64 real-GPU frame cases completed with zero errors. The full captures are in the task's
ignored `.lead/discrete-gpu.log` and `.lead/integrated-gpu.log`.

Conclusion: the real GPU does not rescue the current Vello raster path for camera changes: at
2880×1800 zoom/hover take 1.74–1.88 s without LOD and 448–493 ms with LOD on either adapter.
Retained vectors are much faster (about 1.5–11 ms at 2880×1800), but LOD vectors still reach
12.7 ms on RTX at 1280×800 and 20.8 ms on the AMD adapter. These paths do not establish
120-Hz performance at all tested sizes/interactions; a retained graph-specific GPU path remains
necessary to validate the proposal.
One-minute host load at CPU phase boundaries: 44.81 → 42.34 → 3.16; GPU: 2.78 → 3.34.
120 Hz is not feasible for the tested CPU vector path, including retained geometry + LOD:
2880×1800 pan is 186.30 ms, or 52.06 ms at 0.4 zoom, against 8.33 ms.

Layout (five seeded 2k/10k runs): iteration **0.445/0.471 ms** (run medians),
settle **1296.568/1316.681 ms**, median 2,464 iterations; all five reached the annealed
mean velocity² < 1e-5 (world-px/iteration)² for 12 iterations, alpha 0.18 → 0.0009.
This cools motion, not force equilibrium (final RMS force 0.375–0.404); graph quality is unproved.
Worker: 158 published / 156 consumed / 2 coalesced snapshots over 1.311 s (~120 Hz),
copy max 0.004 ms. Then futex-parked, zero added CPU ticks / two seconds (100 Hz clock),
no polling timer; explicit wake joined it. Five tests cover pins, rename/add position continuity,
layout roundtrip, a spring settle and park/wake; post-change visual stability remains unmeasured.

## Proposed architecture

Use plain geometry/layout math in `bead-core`, painters/resources in `bead-render`,
and session lifecycle, data validation and input/event bridging in `bead-ui`.
One general `GraphCanvas` drawing primitive references versioned shared Rust geometry;
its viewport, camera, style revision and interaction revision are small scene state.
Backend hooks cover bounds, clipping, translation, hashing, preparation and damage.
Choose compact retained node/edge instance buffers and a camera uniform for the GPU,
with CPU tiny-skia fallback; update positions only when layout publishes a snapshot.
The measured Vello encoding prototype is not retained GPU instance buffers.
Current wgpu/Vello composites a CPU-baked image; add a graph vector path. Native GL
already draws ordinary vectors; hardware measurements must select/validate backend implementation.
Do not retain a world-sized raster: it has finite pan coverage, blurs during zoom and
costs 3.91 / 19.78 MiB per viewport-sized RGBA copy at the two target resolutions.

Session-owned state holds IDs, endpoints, positions, pins, adjacency and a spatial index.
Luau submits initial data and explicit deltas to a resource handle; retain no duplicate
full Luau/JSON arrays. Validate IDs, finite values, budgets and snapshot generations;
reject updates atomically. Cancel/join on retirement and release backend resources.

One worker owns positions/velocities. Topology/pin/restore changes wake it;
rename/style changes do not. Add near a neighbor and reheat locally without reseeding.
Publish versioned latest-only snapshots through a bounded mailbox and host wakeup;
consume at most once/frame. Never block UI on layout, call Luau from the worker,
or poll while idle. Park at an energy/displacement threshold; cancel on retirement.
Use dependency-free grid repulsion plus springs provisionally; dense/hub/disconnected
quality gates must pass before selecting this over Barnes–Hut. No new dependency.

Cull using inverse camera bounds (including radius/label margins and crossing edges).
Use deterministic edge decimation and lower alpha below theme-configured zoom thresholds;
retain selected/highlighted incident edges, with hysteresis to avoid threshold flicker.
Cache labels independently using the existing text system and the priority policy below.
Mandatory highlighted labels need a design decision against measured limits; never silently omit them.
Unchanged canvas requests no frame/wake. Hover damages old/new overlay bounds; camera
motion damages the viewport. Compare revisions, not every edge, for camera changes.

## Provisional phase 2 contract

`kit.GraphCanvas{nodes, edges, style, onHover, onActivate, onViewChanged}` remains the
accepted starting shape. This table sketches additions, not finalized Luau syntax.

| Consumer requirement | General contract |
|---|---|
| 1 — data/style/incremental | Nodes `{id,label,group,weight,pinned?}`, edges `{from,to,kind,weight?}`; theme style slots; `setData` reconciles by stable ID; label changes preserve positions |
| 2 — highlight/filter/local graph | `highlight={ids}`, `hiddenGroups`, `neighborhood={id,depth}` update masks; hiding never discards layout positions |
| 3 — focus/selection/pinning | Controlled `selected`, `focus(id)` animates camera; drag constrains position immediately and emits pin/position change for host persistence |
| 4 — events/keyboard | `onHover(id\|nil)`, `onSelect(id)`, double click/Enter `onActivate(id)`, `onContextMenu(id,x,y)`, coalesced `onViewChanged{zoom,center}`; arrows choose neighbors, Escape clears |
| 5 — labels | Style policy controls thresholds, priority, truncation and full hover label; no app font/color literals in Rust |
| 6 — reopen/layout | `getLayout()` / `setLayout()` export/import positions by ID plus pin flags; reconcile missing/new IDs; saved positions paint immediately before optional refinement |
| 7 — scale/idle | Retained geometry, LOD and worker parking; measure completed frames against 8.33 ms, not submit time alone |

Rust hit-tests world positions via the spatial index; one canvas focus target routes
pointer/key input and returns IDs to Luau. Coalesce changed hover/view events; distinct
data/layout/view revisions prevent feedback. Composition, menus and actions remain Luau.

## Memory proposal and phase 2 gates

Propose **1 MiB incremental retained feature state**, including layout, snapshot overlap,
ID/adjacency/spatial indexes, bounded labels and graph-specific CPU/GPU geometry.
Reuse the host output buffer; a graph raster cache is excluded from this design.
Layout payload measured 278,000 B (0.265 MiB); process RSS 2024 → 3456 KiB is context,
not a controlled unreclaimable-memory gate. Account Lua copies, peak overlap, worker stack,
text and driver allocations against a control; the 1 MiB cap remains unverified.
Any additional graph raster or a measured total above 1 MiB needs lead approval.

Open: 50 ms / 8 MiB data ingestion, total feature memory, real-GPU timings and adversarial layout quality;
font/label workload and priority caps; precise persistence and accessibility contracts;
callback/action representation, controlled-selection semantics and consumer design approval.
Phase 1 spikes/evidence are ignored; delete seven days after review.

## Phase 2a: engine, graph primitive, CPU path, Luau binding

What exists (after the follow-up simplicity pass):

- `bead_core::graph`: `Layout` (grid-binned repulsion plus degree-normalised springs, as in the
  phase 1 spike; no new dependency) and `Worker` (one thread owning positions/velocities). Its
  one command, `Replace { positions, edges, hot }`, is kept latest-only; `hot` nodes and their
  direct neighbours settle while every other node keeps its exact position. Snapshots go through a
  latest-only mailbox tagged with the command revision they reflect, so the UI discards snapshots
  computed for an older topology; one host wake per pending snapshot, condvar park at zero CPU,
  cancel/join on drop. Settling anneals `alpha` 0.18 → 0.001 (×0.998/step) and stops after 12
  quiet steps (mean velocity² < 1e-5) or 3,000 steps. `Grid` (flat sorted bins) also serves hit tests.
- `bead_render::Primitive::Graph { rect, geometry: Arc<GraphGeometry>, camera, style, hover, selected }`:
  `GraphGeometry` (revision, positions, radii, edge index pairs, each an `Arc` slice, replaced as a
  whole), `Camera` (centre + zoom), `GraphStyle` (theme colours and thresholds) and the CPU
  painter. Hashing and damage compare the geometry identity/revision, camera, style and
  interaction, never every edge. The painter writes anti-aliased hairline edges (Liang–Barsky
  clipped, Wu) and coverage discs straight into the damage strip: no scratch and no retained
  raster, so the 16 MiB repair-scratch guard that failed phase 1 at 2880×1800 is not involved.
  LOD: edges and nodes outside the clip are skipped; below `lod_zoom` (default 0.8) only every
  fourth unfocused edge is drawn, at `far_edge_alpha`; edges of the hovered or selected node always
  draw. Hover and selection damage only the old/new focus discs (plus their edges below
  `lod_zoom`); camera, topology and style changes damage the viewport. The legacy `CpuRenderer`,
  native GL and the Vello comparison kind draw nothing for the primitive; the wgpu femtovg
  renderer draws it through `GraphGpu` (phase 2b below).
- `bead_ui::graph_data`: `ui.graph(nodes, edges)` walks the Luau tables once into one
  `GraphResource` (ids and labels as `Box<str>`, `f32` weight, `Arc<[[u32; 2]]>` edges, an
  id-sorted index and a weight-sorted label priority; 20k nodes / 100k edges caps; duplicate ids,
  missing edge ends and negative or non-finite weights rejected) and returns a handle that
  serializes into the view tree as a token, so the materialized JSON never holds the graph. Equal
  content keeps its token, so an unchanged view still patches in place. Hand-over is native time,
  excluded from the 50 ms markup budget like text measurement, and a build that handed over new
  data runs one full Luau collection afterwards, so the tables do not linger as garbage in an idle VM.
- `bead_ui::graph`: one `Canvas` per `Graph` view node, owned by the `Session` and surviving
  reloads. It shares the `GraphResource` and its edge array with the view node, the worker command
  and the render geometry, holds positions as the worker's latest `Arc<[Point]>` snapshot and
  rebuilds the radius array only when weights or `node_radius` change. Data is reconciled by id:
  renames, weights and style never reach the worker; a topology change sends one `Replace` in which
  kept nodes keep their positions and new nodes start 40 world units from a placed neighbour (a
  fresh graph starts in a deterministic scatter); hover and selection follow ids. Input: hover emits
  `on_hover id|""` on change; a click on a node selects it (ring) and runs `on_select id`; a drag
  pans; the wheel zooms around the pointer. Camera, hover and layout snapshots patch the committed
  scene (`Session::graph_frame`) without running Luau; a snapshot is consumed at most once per frame.
  Labels are ordinary `Text` primitives: hovered > selected > visible nodes by weight once
  `zoom >= label_zoom`, capped by `max_labels` (default 32, at most 256) and the free share of the
  1,024 scene text slots. Host wake: `Session::set_wakeup` (the `bead` host wires it to an empty
  model event).
- `kit.GraphCanvas{ id, graph = ui.graph(nodes, edges), style?, on_hover?, on_select? }` over
  `ui.Graph`; `style` holds palette roles (`node`, `edge`, `hover`, `selected`, `label`) and numbers
  (`node_radius`, `label_size`, `label_zoom`, `max_labels`, `lod_zoom`, `far_edge_alpha`).
- `examples/graph-demo`: a seeded 2k/10k graph built in a pure Luau module and handed over once;
  the status line shows hover and selection; `bead check examples/graph-demo` runs in CI.

Removed by the simplicity pass because neither `kit.GraphCanvas` nor the demo needs them (bring
back with a consumer): the `nodes=`/`edges=` input form (one form remains), node drag/pinning
(`on_pin`, the `Pin` command, `pinned` data), `on_view_changed`, controlled `selected`,
`hidden_groups` (a filtered `ui.graph` replaces it, phase 2c), node groups, and the
`graph_layout`/`set_graph_layout` export/import. Context menus, `focus(id)`, `neighborhood`
masks and LOD hysteresis stay open. Phase 2c brought back `on_activate` and `highlight`.

Rollback: an update rejected while the view builds (for example an edge naming a missing node)
leaves every canvas's topology, positions and worker revision untouched
(`graph::tests::invalid_graph_update_keeps_every_canvas_and_layout`), and every canvas's style is
resolved before any engine changes. A scene the host rejects after a successful stage
(`Session::reject`) is not rolled back in the engines; the next build reconciles them by id again.

Measured (secondary build host, Ubuntu 24.04 container, 8 CPUs / 8 GiB, Rust 1.99.0 release,
ignored `.lead/render-probe.rs` harness;
median/p95 ms over 7 samples, prepare + damage + draw through the tree CPU renderer, one
canvas with 16 fixed synthetic labels; excludes Luau callbacks and Wayland presentation):

| CPU viewport | Zoom | Full | Pan | Zoom change | Hover |
|---|---:|---:|---:|---:|---:|
| 1280×800 | 1.0 | 111.136/111.393 | 111.033/111.646 | 111.106/111.511 | 0.167/0.174 |
| 1280×800 | 0.4 (LOD) | 16.485/16.512 | 16.458/16.498 | 16.603/16.616 | 6.857/6.870 |
| 2880×1800 | 1.0 | 437.736/452.103 | 433.003/446.808 | 431.198/433.623 | 0.208/0.210 |
| 2880×1800 | 0.4 (LOD) | 55.943/57.111 | 56.012/56.350 | 58.264/61.316 | 17.628/17.921 |

Scale is 1 / 2.25 respectively; pan is (4,2), zoom change ×1.01, and hover selects
one visible node. At zoom 1, incident edge pixels do not change, so only the focus ring
is damaged. At zoom 0.4, incident edges become visible and their bounds also need repair.
These completed CPU draws do not meet 120 Hz for camera movement; GPU work remains phase 2b.
The painter has not changed since this table except that highlight dimming is gone.

Memory (phase 2a follow-up). Same method as the checkpoint: five alternating fresh-process
pairs, an empty-graph control (`ui.graph({}, {})`) against the 2k/10k demo, one release probe
binary (ignored `.lead/probe.rs`), CPU renderer, 1280×800, same palette, settled layout and first
completed frame. Unreclaimable = `Pss_Anon + Pss_Shmem + SwapPss`; shmem, swap and DRM were zero.
Host: oyster, Ubuntu 24.04 container, 6 CPUs / 6 GiB, Rust 1.99.0. Medians in KiB:

| Build | Control | Graph | Paired increase |
|---|---:|---:|---:|
| `a4e44671` checkpoint (dev-1) | 6,204 | 23,716 | 17,516 (17.1 MiB) |
| one Rust hand-over, before the simplicity pass | 6,192 | 10,584 | 4,396 (4.29 MiB) |
| simplicity pass + Luau collection after hand-over | 6,204 | 10,508 | 4,344 (4.24 MiB) |
| + `malloc_trim(0)` after hand-over (final) | 6,204 | 7,568 | **1,384 (1.35 MiB)** |

**The 1 MiB budget is still missed by 0.35 MiB**: median 1,384 KiB (1.35 MiB); three pairs
were 1,284–1,384 KiB and two took a +0.9 MiB allocator step (2,216 and 2,376 KiB), the kind of
arena step earlier units saw in control and candidate alike. Floor per component, from two
diagnostic readings later in the same processes (release the markup VM, then `malloc_trim(0)`):

| Component (final build) | KiB |
|---|---:|
| Resident graph state left after both (Rust data, layout worker, canvas, worker stack/arena) | 620 |
| Luau VM residue: pages the markup VM keeps after building the 12k tables (gone only with the VM) | 444 |
| Allocator retention after the hand-over trim (worker churn during the 2.9 s settle, first frame) | 320 |
| **Gated increase** | **1,384** |

Estimated make-up of the 620 KiB for 2k/10k: `GraphResource` ~300 KB (node records 80 KB, id and
label strings ~128 KB with allocator overhead, edges 80 KB, id index and priority 16 KB), layout
worker ~100 KB (positions, velocities and forces 16 KB each, degree 8 KB, grid 24 KB), canvas
~24 KB (latest snapshot, radii) and the worker thread's stack and malloc arena (~150 KB); the hit
grid (24 KB) and labels appear on first hover. Options for the lead (not done): accept ~1.4 MiB;
trim once more after the first settle (removes the last row, so roughly 1.06 MiB, unmeasured);
pack ids and labels into one buffer (~0.12 MiB). The VM residue is inherent to describing the
graph as Luau tables in a VM that stays alive.

What the trim recovered (previous row, same diagnostic readings): of its 4,344 KiB, releasing the
VM removed 560 KiB and `malloc_trim(0)` returned 3,204 KiB of freed pages glibc kept after the Luau
tables and ingest temporaries were freed, leaving 580 KiB. Clean file-backed PSS did not change
beyond noise.

Before this follow-up (`a4e44671`, tagged counting allocator, requested live bytes after one
hover): Luau graph evaluation and module tables 2,127,328; cached JSON/serde view tree 8,396,820;
parsed Rust graph data 277,786; canvas positions, pins, id map and priority 170,077; render
geometry with copied edges and positions 104,000; layout worker 122,000; hit index 89,952; other
renderer/text/view increments 30,868; total 11,318,831 B. The JSON copy and the retained Luau
tables are gone; edges and positions are shared instead of copied.

Ingest (probe, release): session-load thread CPU 35.9 ms median (control 9.2 ms), of which
25.5 ms is evaluating the demo module that builds the 12k Luau tables; the markup VM counter after
the build is 1.56 MB (control 1.09 MB; 4.9 MB before the collection). The settled snapshot arrived
2.94 s after load on oyster (median of five; 1.62 s on dev-1 at the checkpoint).

Checks for this follow-up (commands in the unit's ignored `.lead/run9.sh` and `.lead/run13.sh`):
fmt; warnings-denied Clippy for the workspace and for `bead-render --features wgpu`; the whole
bead-core (42), bead-render (133), bead-ui (205) and bead-kit test binaries; `bead check
examples/graph-demo`; the wgpu tests on llvmpipe (Mesa 25.2.8), including
`gpu::tests::wgpu_path_draws_graphs_after_the_layer_blit`; and the full `ci.yml` replica. A
private headless sway capture (debug `bead run`, pixman, 1280×800) waited for the layout to
settle, then hovered four nodes (label and ring, status `hover n1050`, `n1160`, `n1005`, `n156`)
and clicked one (status `selected n1050`), without `UI_ERROR`. Logs, probes and screenshots
stay in `.lead/`.

## Phase 2b: retained GPU graph path

`bead_render::graph_gpu::GraphGpu` (feature `wgpu`) keeps the graph in three GPU storage
buffers (positions 8 B/node, node attributes 12 B/node: radius, RGBA8, highlight flag,
edges 12 B/edge: two indices, RGBA8) plus one 112-byte uniform (camera transform, viewport,
line width, LOD/dim flags and alphas, hover/selected/focus indices and ring colours).
A frame writes that uniform and records two instanced draws of six vertices each: edges as
line quads (vertex shader reads both endpoints from the position buffer, fragment coverage
from the distance to the segment, 1 px anti-aliasing both across and at the butt ends) and
nodes as quads with an SDF disc (fill over an optional hover/selected/focus ring, radius clamped to
1–64 device px like the CPU painter). Far-LOD decimation (every fourth unfocused edge at
`far_edge_alpha`) and highlight dimming are decided in the vertex shader from the retained flag, so no CPU culling, tessellation or re-upload happens on
pan, zoom, hover, selection or LOD changes; viewport clipping is the GPU's. Output is
premultiplied source-over into the caller's render pass, scissored to the canvas rectangle.
Labels are not drawn here; they stay text primitives above the graph (phase 2c).

Backend: wgpu, because it is the only Bead GPU path that hands out a device for custom
pipelines; native-gl exposes femtovg's canvas API only, so the same buffers and shader logic
can follow there once a raw GL binding is allowed. The CPU painter from phase 2a stays the
fallback.

Updates: `upload(device, positions, nodes, edges)` per data revision (reallocates, validates
counts and edge indices); `set_positions(queue, first, positions)` per layout snapshot
(16,000 B for 2k nodes, or a range for a drag); `set_nodes(queue, first, nodes)` per
mask/style change (24,000 B); `frame(queue, Frame)` per frame (112 B); `draw(&mut pass)`.
Damage: camera changes repaint the canvas rectangle; hover/selection are a uniform write and
the same repaint, which the numbers below show is far below one 120 Hz frame.

Bench (`cargo test -p bead-render --features wgpu --release graph_gpu -- --ignored --nocapture`):
the same seeded 2k/10k graph, logical viewport 1280×800 at scale 1 and 2.25, 200 frames per
case after 10 warm-ups, each frame waited to completion; cells are median/p95 ms. `cpu` is
uniform write + draw recording + submit, `done` is submit to completion, `gpu` is the pass
from its timestamp queries. Cases: full repaint, pan (4,2) per frame, zoom ×1.01 alternating,
hover toggling one node, and far-pan (zoom 0.4 below `lod_zoom`, a quarter of the edges).
Upload of all buffers 0.092 ms and a full position snapshot 0.149 ms on the GTX (0.207 / 0.591 ms
on llvmpipe). GPU timestamps were available on both adapters.

| Host / adapter | Resolution | Full | Pan | Zoom | Hover | Far-pan |
|---|---|---:|---:|---:|---:|---:|
| oyster llvmpipe (LLVM 20.1.2, Mesa 25.2.8, 8 CPUs, software, not representative) cpu | 1280x800 | 0.042/0.061 | 0.038/0.052 | 0.041/0.062 | 0.041/0.060 | 0.033/0.049 |
| same, done | 1280x800 | 57.05/70.53 | 52.95/68.18 | 57.43/69.96 | 57.16/69.71 | 13.67/16.54 |
| same, cpu | 2880x1800 | 0.065/0.073 | 0.065/0.075 | 0.066/0.074 | 0.065/0.073 | 0.050/0.064 |
| same, done | 2880x1800 | 135.23/163.84 | 124.69/160.84 | 134.59/162.73 | 137.34/164.13 | 19.66/21.47 |
| GTX 1650 SUPER (NVIDIA 595.91.07, Vulkan, containerised) cpu | 1280x800 | 0.050/0.059 | 0.051/0.061 | 0.051/0.062 | 0.052/0.062 | 0.050/0.059 |
| same, done | 1280x800 | 1.087/1.105 | 0.937/1.074 | 0.973/0.985 | 0.971/0.983 | 0.205/0.214 |
| same, gpu | 1280x800 | 0.985/0.990 | 0.829/0.957 | 0.861/0.866 | 0.860/0.865 | 0.100/0.102 |
| same, cpu | 2880x1800 | 0.053/0.069 | 0.053/0.062 | 0.052/0.066 | 0.050/0.063 | 0.050/0.059 |
| same, done | 2880x1800 | 2.846/2.869 | 2.628/2.850 | 2.856/2.874 | 2.850/2.871 | 0.409/0.432 |
| same, gpu | 2880x1800 | 2.718/2.729 | 2.486/2.714 | 2.720/2.734 | 2.720/2.735 | 0.299/0.301 |

The CPU side is 0.04–0.07 ms per frame in every case, so the frame cost is fragment work
only (about 10k line quads of 1.5 px half-width plus 2k discs). On a software rasterizer that
work is the whole budget; on the GTX 1650 SUPER every case completes in 0.2–1.1 ms at
1280×800 and 0.4–2.9 ms at 2880×1800 (p95), under the 8.33 ms frame with headroom for
labels and compositing, and pan/zoom/hover cost the same as a full repaint, as designed.
Phase 1b's LOD-vector anomaly (12–21 ms at 1280×800, faster at 2880×1800) is specific to
Vello's tile pipeline for its LOD scene and is not touched by this path: here the far-LOD case
is 4–7× cheaper than the full case at both sizes, proportional to fragments, with the same
CPU cost.

Memory for 2k/10k: GPU 160,112 B (0.153 MiB, `gpu_bytes()`); CPU-side retained state is the
handles and the last `Frame` (about 150 B); uploads stage a transient byte vector of the
same size as the buffer written. Against the 1 MiB proposal this leaves about 0.85 MiB for
layout, indexes and labels. No raster cache is involved.

Integration contract for phase 2a (`Primitive::Graph { rect, geometry, camera, style, hover, selected }`):
the GPU renderer keeps one `GraphGpu` per canvas resource; when `geometry.revision` (or the
`Arc` identity) changes it calls `upload` with `nodes[i] = Node { radius: radii[i], color:
style.node, flags: HIGHLIGHTED from the mask }` and `edges` from the index pairs with
`style.edge`; a layout snapshot with unchanged topology calls `set_positions` only; a mask or
style change calls `set_nodes`. Per frame it calls `frame` with `rect` offset by the scene
offset, the output scale and size, `camera.center`/`camera.zoom`, `far = camera.zoom <
style.lod_zoom`, `any_highlighted = !geometry.highlight.is_empty()`, the style
alphas and colours and `hover`/`selected`, then `draw` in the pass that follows the retained
layer blit. The CPU bake must skip `Primitive::Graph` when this path draws it, and the layer
content key must not include the camera.

Hooked up (phase 2a follow-up): `bead_render::gpu::GpuRenderer` (wgpu femtovg kind) keeps one
`GraphGpu` per `Primitive::Graph` (by primitive index) and syncs it from the shared geometry
arrays by identity: a new edge array, node count or edge colour re-uploads everything, a new
position array (a layout snapshot) calls `set_positions`, a new radius array or node colour calls
`set_nodes` (as does a new highlight mask, phase 2c); hover, selection, focus and the camera
are the per-frame uniform. The graphs are drawn in
one extra render pass (`LoadOp::Load`) on the target after femtovg's layer blit, scissored to each
canvas. `prepare` bakes the retained layer from a copy of the scene whose graph primitives are
transparent rectangles, so neither the layer key nor its pixels follow the graph or the camera,
and the CPU painter never runs on this path. Labels draw above the graph through an overlay
bake (phase 2c). The Vello comparison kind and native GL draw nothing for
the primitive. `gpu::tests::wgpu_path_draws_graphs_after_the_layer_blit` (any Vulkan adapter,
llvmpipe in CI containers) proves the bake stays empty and camera-independent and that the drawn
pixels, including the hover ring, land inside the canvas rectangle.

## Phase 2c: activation, highlight, keyboard focus, labels above the GPU graph

For a reader-beside-graph view: the graph picks what the reader shows, and the reader's
context lights up part of the graph.

- `on_activate = ui.action(...)` runs with the node id on a double-click (a second click on the
  same node within 400 ms) and on Enter for the keyboard-focused node. `on_select` still runs only
  for clicks (and with `""` when Escape clears the selection). A graph's `on_activate` is not the
  node's generic action, so generic activation and accessibility never call it without an id.
- `highlight = {ids}`, or a bound list store of `{id = ...}` records (store lists hold records):
  those nodes and every edge touching them draw at full strength, everything else at
  `bead_render::graph::DIM_ALPHA` (70/255, one constant); nil or empty dims nothing, unknown ids
  are ignored, and highlighted edges survive far-LOD decimation. Paint-only: the canvas turns the
  set into a per-node mask on the shared geometry; positions, radii and the edge array keep their
  identity and the layout worker hears nothing. The GPU path then rewrites only the node
  attribute buffer (`set_nodes`, 24,000 B for 2k nodes); the CPU path repaints the canvas once,
  as for a style change, with no other geometry work. Tests:
  `graph::tests::activation_highlight_and_the_keyboard_walk` (ui),
  `graph::tests::highlight_keeps_its_nodes_and_edges_and_dims_the_rest` (CPU painter),
  `gpu::tests::graph_slot_rewrites_only_the_mask_for_a_highlight` and
  `graph_gpu::tests::draws_discs_edges_rings_and_dims_outside_the_highlight` (wgpu).
- Keyboard: a graph canvas is focusable (Tab, or a click on a node, which does not paint the
  canvas focus ring). Arrows move the focused node: inside the 90° cone around the arrow (the
  component along the arrow at least the one across it) the nearest node sharing an edge wins,
  else the nearest node in the cone; an empty cone keeps the focus. With nothing focused, an arrow
  starts at the node nearest the camera centre. Enter runs `on_activate`; Escape clears focus and
  selection. The focused node gets a ring in `style.focus` (palette `primaryContainer`; a hover or
  selection ring on the same node wins) and its label is always shown, one beyond `max_labels`.
  The camera moves only when the focused node is outside the canvas or within 16 px of its edge:
  the centre eases onto the node over 240 ms, patched into the scene per frame without Luau
  (`Session::graph_deadline` wakes the host).
- Hidden groups: a filtered `ui.graph(...)` is enough, so `hidden_groups` stays out. Survivors keep
  their exact positions (reconciled by id) and the worker's `Replace` marks no node hot, so the
  layout is settled at once: one snapshot of the same positions, no step and no new settle
  (`graph::tests::a_filtered_graph_keeps_survivors_in_place_without_a_new_settle`). Nodes that come
  back are new to the canvas: they start 40 world units from a placed neighbour and settle locally
  with their neighbours, not at their old spot.
- Labels above the GPU graph: `GpuRenderer::prepare` bakes two layers around the graph pass. The
  base holds everything up to the last graph (graphs as transparent rectangles) and nothing past
  the last scene layer, so labels moving with the camera never change its key; the overlay holds
  everything after the last graph (labels, the focus ring, later widgets), sized to their bounds,
  and is blitted after the graphs. A pan with labels visible re-bakes only the overlay. Cost: the
  overlay is one more RGBA layer and texture of those bounds on the GPU path only (up to
  1280×800×4 = 4 MB each when labels spread over a full-window canvas). `graph_gpu`'s hidden flag
  and `to_screen` are deleted; the highlight flag is wired, and the uniform carries the focus
  index and colour (112 bytes). `gpu::tests::wgpu_path_draws_graphs_after_the_layer_blit` checks
  that a primitive after the graph lands above it and that neither the camera nor a moved label
  changes the base key.

Memory, same method as phase 2a (five alternating fresh-process pairs of an empty-graph control
against the 2k/10k demo, release, CPU renderer, 1280×800, settled layout and a completed frame;
`Pss_Anon + Pss_Shmem + SwapPss`; the phase 2a memory host, Ubuntu 24.04 container, 6 CPUs / 6 GiB). The phase 2a
probe was not kept, so a new ignored probe drives the folder host (`FolderView::load` and
`check_render`, staged snapshots until 1.5 s pass without one, the latest frame rendered on both
sides) and measured the previous main and this phase in one session: paired increase median
1,408 KiB before (pairs 1,328–1,460; phase 2a recorded 1,384) and 1,452 KiB after (1,396–1,484),
so **+44 KiB**. Clean file-backed PSS did not grow. The GPU overlay is not on this path.

Checks (secondary build host, `nacre-ci/ubuntu-bead` image, Rust 1.99.0): the full `ci.yml`
replica (fmt, boundaries, warnings-denied Clippy, `cargo test --workspace` 676 passed, feature
checks, minimal hosts, `bead check` on all six example apps, private service lifecycles,
cargo-deny for every crate); Clippy for `bead-render --features wgpu`; the wgpu tests on lavapipe.
A private headless sway capture (debug `bead run`, pixman, 1280×800, input from a kept-alive
virtual pointer and keyboard, since synthetic `swaymsg` clicks reach no client without a pointer
device) waited for the layout to settle, then hovered and clicked a hub (label, ring, `selected
n1250`), walked right and down with the arrows (focus ring and label on n51, then n826, while
n1250 kept its selection ring), pressed Enter (`activated n826`; the hubs and their edges at full
strength, the rest dimmed, labels above), Escape (`cleared`) and double-clicked the hub
(`activated n1250`), without `UI_ERROR`.

Lines (product / tests, against the previous main): bead-ui +249/−17 / +194/−4, bead-render
+183/−56 / +140/−15, bead-kit +6/−4 (comment), bead +1.
