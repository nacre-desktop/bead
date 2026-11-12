# 0232 — GraphCanvas feasibility

Status: draft; phase 1 investigation, not an approved component/API.

Target: 2,000 nodes / 10,000 edges at 120 Hz (8.33 ms/frame), including pan/zoom; quiescent when unchanged.
Applications own content, persistence and theme policy; phase 2 waits for approved design.

## Evidence and limitations

Release, offscreen measurements use dev-1 (Ryzen 9 9950X, 32 host threads),
Ubuntu 24.04, Rust 1.99.0/workspace release profiles, fresh 6-CPU / 12-GiB slot; target inside.
Seven samples/case; median and nearest-rank p95 (the maximum at n=7), in ms.
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
GPU = **llvmpipe, not representative** (LLVM 20.1.2/Mesa 25.2.8); desktop SSH timed out twice.
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
Spec/index only; no production module/API. Spikes/evidence are ignored; delete seven days after review.
