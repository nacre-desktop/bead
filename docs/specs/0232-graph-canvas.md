# 0232 — GraphCanvas feasibility

Status: phase 2a checkpoint blocked by the 1 MiB memory gate (measured +17.105 MiB below); the Luau contract stays provisional until the consumer's design arrives; phase 2b adds the retained GPU path and validates 120 Hz on real GPUs.

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

What exists:

- `bead_core::graph`: `Layout` (grid-binned repulsion plus degree-normalised springs, as in the
  phase 1 spike; no new dependency), `Worker` (one thread owning positions/velocities, commands
  `Replace { positions, pinned, edges, hot }` and `Pin { node, position, pinned }`, latest-only
  `Snapshot` mailbox with command sequence checks (old topology/drag/restore snapshots are discarded),
  one host wake per pending snapshot, condvar park at zero CPU, cancel/join on
  drop). Settling anneals `alpha` 0.18 → 0.001 (×0.998/step) and stops after 12 quiet steps
  (mean velocity² < 1e-5) or 3,000 steps. A per-node `heat` lets a local change (new node, unpin)
  move only the touched node and its neighbours; frozen nodes keep their exact positions.
- `bead_render::Primitive::Graph { rect, geometry: Arc<GraphGeometry>, camera, style, hover, selected }`
  with `bead_render::graph`: `GraphGeometry` (revision, positions, radii, edge index pairs, hidden and
  highlighted flags, replaced as a whole on change), `Camera` (centre + zoom), `GraphStyle`
  (theme colours and thresholds) and the CPU painter. Hashing and damage compare the geometry
  revision, camera, style and interaction, never every edge. The painter writes anti-aliased
  hairline edges (Liang–Barsky clipped, Wu) and coverage discs straight into the damage strip:
  no scratch and no retained raster, so the 16 MiB repair-scratch guard that failed phase 1 at
  2880×1800 is not involved. LOD: edges and nodes outside the clip are skipped; below
  `style.lod_zoom` (default 0.8) only every fourth unfocused edge is drawn at `far_edge_alpha`;
  edges touching the hovered, selected or highlighted nodes always draw. Hover and selection
  change only the primitive's interaction fields; damage covers old/new focus nodes and affected
  incident edges. Camera, topology and style changes damage the viewport.
  Labels are ordinary `Text` primitives appended by the session (hovered > selected >
  highlighted by weight > visible nodes by weight once `zoom >= label_zoom`), capped by
  `max_labels` (default 32, at most 256) and the remaining shared 1,024 scene text slots.
  The legacy `CpuRenderer` and native GL draw nothing for the primitive and do not fail on it;
  the wgpu femtovg renderer draws it through the retained GPU path (below).
- `bead_ui::graph_data`: the data crosses the Luau boundary once. `ui.graph(nodes, edges)` walks the
  Luau tables straight into one `GraphResource` (ids and labels as `Box<str>`, interned group index,
  `f32` weight, `Arc<[[u32; 2]]>` edges, an id-sorted index and a weight-sorted label priority;
  20k nodes / 100k edges caps, duplicate ids, missing edge ends and non-finite weights rejected
  there) and returns a handle that serializes into the view tree as a small token, so the
  materialized JSON tree never holds the graph. Equal content keeps its token, so an unchanged
  view still patches in place; a stale token is a diagnostic, and a handle built in a pure
  module (the demo does this) is cached with the module, so the Luau tables are garbage after
  hand-over. `kit.GraphCanvas` accepts `graph = ui.graph(...)` or `nodes`/`edges` (then it hands
  over per build).
- `bead_ui::graph`: one `Canvas` per `Graph` view node, owned by the `Session` and surviving view
  reloads. The canvas shares the `GraphResource` and its edge array with the view node, the
  worker command and the render geometry (no copies after ingest), holds positions as the
  worker's latest `Arc<[Point]>` snapshot, and reuses the radius/mask arrays until attributes
  change. Data is reconciled by id: a rename, weight,
  group, highlight or style change never reaches the worker; node/edge/pin changes send one
  `Replace` where kept nodes keep their positions, new nodes start 40 world units from a placed
  neighbour (or in the scatter) and settle locally. A flat sorted world-space grid
  (`bead_core::graph::Grid`, shared with the layout step) narrows hit tests;
  position and radius changes invalidate it. Interaction indices and runtime pins follow stable IDs. Pointer:
  press on a node then drag moves and pins it (`Pin` command; `on_pin{id,x,y}` on release), press
  on empty canvas then drag pans, wheel zooms around the pointer (`on_view_changed{zoom,cx,cy}`),
  click selects (`on_select id`), a second click within 400 ms activates (`on_activate id`), hover
  emits `on_hover id|""` on change. Camera, hover and layout snapshots patch the committed scene
  directly (`Session::graph_frame`), without running Luau; a snapshot is consumed at most once per
  frame. `Session::graph_layout(id)` / `set_graph_layout(id, map)` use the full scoped canvas ID
  (for example `root.canvas`) and export/import positions and
  pin flags by id. Host wake: `Session::set_wakeup` (the `bead` host wires it to an empty model
  event).
- `kit.GraphCanvas{ id, graph | nodes, edges, style?, highlight?, hidden_groups?, selected?, on_hover?,
  on_select?, on_activate?, on_pin?, on_view_changed? }` over the `ui.Graph` widget
  (`selected` maps to the widget's `selected_node`, the data to its `graph` token). Keyboard navigation, context menu, focus
  animation, `neighborhood` masks and hysteresis at the LOD threshold are still open.
- `examples/graph-demo`: seeded 2k/10k graph generated in Luau, status line with hover, select,
  activate, pin and view events; `bead check examples/graph-demo` runs in CI.

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
Geometry arrays occupy 104,000 bytes, which excludes data binding and worker state.

Ingestion succeeded in all five final 2k/10k runs with the unchanged 50 ms Luau and
8 MiB VM limits. Total session-load thread CPU was **49.021 ms median**, 47.828–49.277 ms;
a separate profiled load spent **27.010 ms** in markup (9.670 ms evaluation, 16.533 ms
serialization). The post-build markup VM counter was **3,227,724 B (3.078 MiB)**;
the retained markup-values counter was **3,441,143 B**. These counters are not total
process memory. The last settled snapshot arrived **1.615 s median / 1.623 s max** after
load began (five runs, consumed at 8 ms intervals). Worker tests verify no added Linux
CPU ticks over a bounded 200 ms park interval and joining on drop.

**Memory gate: FAIL; do not accept phase 2a under the existing budget.** Five alternating
fresh-process empty-canvas/2k-10k pairs used the same release binary, CPU renderer,
1280×800 output, palette and first completed frame, after settling. Median unreclaimable
memory was **6,204 KiB control / 23,716 KiB graph**; median paired increase was
**17,516 KiB = 17.105 MiB**, versus the **1 MiB** proposal. Every pair exceeded the cap.
This is `Pss_Anon + Pss_Shmem + SwapPss`; shmem/swap were zero and no DRM allocation is
used by this CPU proof. Clean file-backed PSS increased by a separate median **312 KiB**.
This measures settled retention, not peak ingestion or a GPU/Wayland process comparison.
The Luau module tables and cached raw JSON remain alongside parsed Rust data and geometry;
a geometry-only byte count therefore substantially understates the feature cost. Meeting
the cap requires reworking data ownership/import before acceptance; it has not been raised.
Transactional rollback of graph engines after a rejected staged scene also needs a proof
before this provisional API is accepted.

The full Ubuntu CI recipe was exercised: formatting, warnings-denied workspace Clippy,
workspace tests, renderer feature configurations, all five folder examples, both minimal
hosts, private service/tray/clock proofs, and per-manifest cargo-deny checks. Initial fixture
and lint failures were corrected and the failed steps rerun successfully. The final startup
wake fix additionally passed Clippy and all **189 bead-ui tests**. A private headless sway
capture was inspected: the graph, highlighted node, **Note 813** hover label and matching
status text rendered without `UI_ERROR`. GPU rendering/presentation and interactive 120 Hz
remain phase 2b work. Logs, probes and screenshots stay in the unit's ignored `.lead/`;
only this summary is committed.

## Phase 2a memory ownership follow-up: baseline

Measured before implementation changes at `a4e44671`: Ubuntu 24.04, Rust 1.99.0 release,
6-CPU/12-GiB container, same 1280×800 graph-demo and empty-graph control. Three fresh
alternating pairs used a tagged counting allocator; the graph also received one hover
to populate its hit index and label. These are median paired **requested live bytes**,
not RSS, and exclude the meter's separately counted headers/alignment and the identical
4,096,000-byte output buffer. The production five-pair PSS baseline above is unchanged.

| Retained allocation owner after hover/render | Incremental live bytes |
|---|---:|
| Luau graph evaluation/module allocations | 2,127,328 |
| Cached JSON/serde view tree | 8,396,820 |
| Parsed Rust graph data/IDs/labels/groups/edges | 277,786 |
| Canvas positions, pins, ID map and priority order | 170,077 |
| Render geometry (including copied edges/positions) | 104,000 |
| Layout worker positions/pins/edges/velocities/heat | 122,000 |
| Hit spatial index | 89,952 |
| Label primitive strings | 16 |
| Renderer/text-cache increment | 16,829 |
| Other view/action allocation increment | 14,023 |
| **Total (without double-counting the Lua gauge)** | **11,318,831** |

The mlua 0.12.2 allocator calls Rust `std::alloc`; its `used_memory()` gauge therefore
overlaps these counts. Loading the graph module added **2,115,936 B** to that gauge.
Post-collection VM usage was **3,209,844 B graph / 1,090,708 B control**; collection did
not remove module-owned tables. Releasing the entire markup VM removed the graph-eval
bucket, but retained JSON remained **8,396,820 B** above control. The old cache charge
of 3,441,143 B materially understated the actual JSON allocations.

The mailbox held no unread snapshot after settling; its tagged peak was **32,032 B**
(two 16,016-byte snapshots during replacement). Layout bins/step temporaries returned
to zero live bytes, with a **143,312 B** tagged peak; geometry peaked at **208,288 B**.
Peaks are allocation-site maxima, not additive simultaneous peaks. The meter's realloc
implementation allocates/copies/frees, so peak/PSS results are diagnostic only. The
requested live totals show that Lua tables and the cached JSON tree dominate; they do
not establish a graph-data floor above 1 MiB. Raw profiles/instrumentation stay ignored.

## Phase 2b: retained GPU graph path

`bead_render::graph_gpu::GraphGpu` (feature `wgpu`) keeps the graph in three GPU storage
buffers (positions 8 B/node, node attributes 12 B/node: radius, RGBA8, hidden/highlight flags,
edges 12 B/edge: two indices, RGBA8) plus one 96-byte uniform (camera transform, viewport,
line width, LOD/dim flags and alphas, hover/selected index, hover/selected/highlight colours).
A frame writes that uniform and records two instanced draws of six vertices each: edges as
line quads (vertex shader reads both endpoints from the position buffer, fragment coverage
from the distance to the segment, 1 px anti-aliasing both across and at the butt ends) and
nodes as quads with an SDF disc (fill over an optional hover/selected ring, radius clamped to
1–64 device px like the CPU painter). Hidden nodes and their edges, far-LOD decimation
(every fourth unfocused edge at `far_edge_alpha`) and highlight dimming are decided in the
vertex shader from the retained flags, so no CPU culling, tessellation or re-upload happens on
pan, zoom, hover, selection or LOD changes; viewport clipping is the GPU's. Output is
premultiplied source-over into the caller's render pass, scissored to the canvas rectangle.
Labels are not drawn: `to_screen(world)` gives the logical-px position of a node under the
last frame (the same affine the shader uses) for the existing text path and its priority cap.

Backend: wgpu, because it is the only Bead GPU path that hands out a device for custom
pipelines; native-gl exposes femtovg's canvas API only, so the same buffers and shader logic
can follow there once a raw GL binding is allowed. The CPU painter from phase 2a stays the
fallback.

Updates: `upload(device, positions, nodes, edges)` per data revision (reallocates, validates
counts and edge indices); `set_positions(queue, first, positions)` per layout snapshot
(16,000 B for 2k nodes, or a range for a drag); `set_nodes(queue, first, nodes)` per
mask/style change (24,000 B); `frame(queue, Frame)` per frame (96 B); `draw(&mut pass)`.
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

Memory for 2k/10k: GPU 160,096 B (0.153 MiB, `gpu_bytes()`); CPU-side retained state is the
handles and the last `Frame` (about 150 B); uploads stage a transient byte vector of the
same size as the buffer written. Against the 1 MiB proposal this leaves about 0.85 MiB for
layout, indexes and labels. No raster cache is involved.

Integration contract for phase 2a (`Primitive::Graph { rect, geometry, camera, style, hover, selected }`):
the GPU renderer keeps one `GraphGpu` per canvas resource; when `geometry.revision` (or the
`Arc` identity) changes it calls `upload` with `nodes[i] = Node { radius: radii[i], color:
style.node, flags: HIDDEN/HIGHLIGHTED from the masks }` and `edges` from the index pairs with
`style.edge`; a layout snapshot with unchanged topology calls `set_positions` only; a mask or
style change calls `set_nodes`. Per frame it calls `frame` with `rect` offset by the scene
offset, the output scale and size, `camera.center`/`camera.zoom`, `far = camera.zoom <
style.lod_zoom`, `any_highlighted = geometry.highlighted.iter().any(|h| *h)`, the style
alphas and colours and `hover`/`selected`, then `draw` in the pass that follows the retained
layer blit. The CPU bake must skip `Primitive::Graph` when this path draws it, and the layer
content key must not include the camera. Text labels use `to_screen` per labelled node.

Hooked up (phase 2a follow-up): `bead_render::gpu::GpuRenderer` (wgpu femtovg kind) keeps one
`GraphGpu` per `Primitive::Graph` (by primitive index) and syncs it from the shared geometry
arrays by identity: a new edge array or node count (or edge colour) re-uploads everything, a new
position array calls `set_positions`, new radius/mask arrays or a node colour call `set_nodes`;
hover, selection and the camera are the per-frame uniform. The graphs are drawn in one extra
render pass (`LoadOp::Load`) on the swapchain texture after femtovg's layer blit, scissored to
each canvas. `prepare` bakes the retained layer from a copy of the scene whose graph primitives
are transparent rectangles, so the layer key and pixels ignore the graph and the camera; labels
stay `Text` primitives placed by the session with the same affine as `GraphGpu::to_screen`, so
a pan still re-bakes the layer for the moved labels (a label overlay layer is the follow-up).
The Vello comparison kind and native GL still draw nothing for the primitive.
`gpu::tests::wgpu_path_draws_graphs_after_the_layer_blit` (any Vulkan adapter, llvmpipe in
CI containers) proves the bake stays empty and camera-independent and that the drawn pixels,
including the hover ring, land inside the canvas rectangle.
