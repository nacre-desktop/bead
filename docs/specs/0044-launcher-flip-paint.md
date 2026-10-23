# B0044 — Launcher grid <-> list flip: one commit, order-aware damage

Status: **Review** (Claude worker, branch `task/b0044-flip-paint-20261023`). Follows B0041, which left the BackSpace
(list -> grid) key at 11.4 ms UI CPU against Gate E (<= 8 ms) and named "scene/paint reuse below markup" as the way
forward. Consistent with ADR 0005. No new ADR.

## Where the 11.4 ms went

Harness: `spikes/typing-proof.py` (320 fixture apps, CPU renderer, 2880x1800 @1.5, `a`/BackSpace alternate), headless sway
in an Arch container on dev-2 (`--cpus=8 --memory=12g`), pixman. Stage run (`BEAD_TYPING_PROFILE=1`, thread-CPU sums over
the key window, instrumented so the total is above the clocks-only number), control `ca4c19c`:

| Stage (BackSpace key) | control ms | candidate ms |
| --- | ---: | ---: |
| markup (eval + serde + setup) | 5.22 (1.95 + 2.43 + 0.56) | 2.65 (1.00 + 1.20 + 0.32) |
| view_ir | 1.52 | 0.76 |
| layout | 0.97 | 0.48 |
| bake | 0.83 | 0.68 |
| menu query (actor) | 0.52 | 0 |
| compose (paint) | 2.86 (rounded 1.42, text 0.64, image 0.50) | 2.02 (rounded 0.49, text 0.66, image 0.53) |
| key total (instrumented p50) | 12.70 | 7.96 |

Two causes, neither of them "all 320 tiles" or "tile rasters" (the grid is virtualised, and only 2 rasters are rebuilt per
key because the retained raster cache already hits):

1. **Double commit.** BackSpace to the empty query kept the 64-row result model (row 0 selected) and scheduled a query. The
   first commit drew the grid from that stale model (markup 2.7 ms + compose 3.2 ms), the reply landed a few ms later and
   started a second full stage whose worker CPU overlaps the first commit's presentation window. The control builds the
   markup twice per BackSpace (5.22 ms of markup vs 2.65 ms here).
2. **Damage by paint-slot index.** `FrameDamage` compared the old and new frame item by index. A grid has more items than a
   list, so every later item (the search box) moved to another slot and counted as changed: a 1608x79 px rectangle through
   the slow antialiased rounded path although only 213x38 px changed. Hashing every primitive with three `serde_json`
   allocations (bake, frame key, damage key) added to it.

## Changes (product about +230 / -30 lines in 3 files; tests +72)

1. `nacre-shell/src/launcher.rs`: `View.grid` keeps the last settled grid result (empty query, apps) as `Arc` clones of the
   rows and metadata. Returning to the grid with an unchanged source stamp applies it like a reply in the same projection:
   no stale intermediate commit, no query. Selection is cleared exactly as after a reply, favourites are re-applied (the
   favourites block runs after the restore), and activation or a `stale_revision` error drops it. It lives and dies with the
   `Menu`/panel worker; in grid mode it is the same `Arc` as `view.rows` (no extra memory), in list mode it keeps the 320 grid
   rows alive. Test `returning_to_the_grid_reuses_the_settled_result_in_one_projection`.
2. `bead-render/src/damage.rs`: `align()` pairs old and new items by identical pixels: common prefix and suffix first, then
   a longest-common-subsequence over a differing middle bounded to 65 536 cells (128 KiB transient), positional pairing as
   before inside each unmatched run, surplus is insertion/removal. Items that only changed paint slot stay clean. The
   existing ordered-pixel damage test (stale-pixel equality for insert, remove, reorder, append at 3 scales) still passes;
   new test `an_insert_does_not_dirty_identical_items_that_only_changed_paint_slot`.
3. `bead-render/src/tree.rs`: `hash_primitive` hashes every field bit-exactly without allocating; it replaces the three
   `serde_json::to_vec` calls per primitive in `prepare` and the one in damage. The per-primitive signature is computed once
   (it was twice), and each icon's bytes are hashed once per prepare instead of once per tile.
4. `spikes/typing-proof.py`: `BEAD_TYPING_PAUSE` (seconds between keys, default unchanged 0.04) for the paused lane.

No golden changed.

## Numbers

Same harness, clocks-only (`BEAD_TYPING_PROFILE=clocks`), 60 keys per run, control `ca4c19c` against this branch from the
same toolchain, alternating runs, UI CPU ms, median of run medians (first key: median of run values).

| Lane / key | control | candidate | gate |
| --- | ---: | ---: | --- |
| default p50 (grid <-> list), 5 pairs | 10.86 | **6.87** | <= 8 ok |
| BackSpace (list -> grid), 5 pairs | 11.79 (11.7-12.9) | **7.51** (7.08-8.42; 1 of 5 above 8) | <= 8 ok (median) |
| `a` (grid -> list) | 5.95 | 4.82 | ok |
| first key | 7.84 | 6.21 | <= 8 ok |
| default p99 CPU / presented p99 | 14.24 / 10.81 | 10.23 / 10.23 | presented <= 16.7 ok |
| steady list -> list p50, 5 pairs | 1.21 | 1.19 | no regression |
| changed-results list p50, 3 pairs | 1.78 | 1.79 | no regression |
| BackSpace with 500 ms between keys, 3 pairs | 13.10 | 8.00 (7.94-8.04) | edge: at the gate |

Commits per BackSpace: 2 -> 1; queries 1 -> 0. After `a`/BackSpace/`a`/BackSpace and after five keys the 2880x1800 `grim`
screenshots of control and candidate are pixel-identical (grid and list state).

Memory (CPU renderer; launcher-proof in niri nested in headless sway with software GL, typing proof in headless sway; 5
alternating pairs, PSS+DRM MiB, median; relative gate <= +0.5 MiB):

| Phase | control | candidate | delta |
| --- | ---: | ---: | ---: |
| bar, launcher closed idle | 23.93 | 23.83 | -0.1 |
| first 1 s after open (cycles 1/2/3) | 44.95/45.07/44.96 | 45.17/45.11/45.12 | +0.2/+0.0/+0.2 |
| after close, settled (cycles 1/2/3) | 25.37/25.40/25.47 | 25.30/25.41/25.51 | -0.1/+0.0/+0.0 |
| open, 12 s idle after 61 keys (ends in list = cache held; anon) | 50.23 (19.88) | 48.39 (17.85) | -1.8 (-2.0) |

Pass. The lower typing figure is the removed second commit's transient allocations; the grid cache itself is shared rows.

CI: the whole `check` job of `.github/workflows/ci.yml` (fmt, clippy `-D warnings`, service boundary, workspace tests with
goldens, trace tests, feature checks, Luau schema, spike/packaging tests, services and tray lifecycle, cargo-deny) in an Arch
container on dev-1: all steps passed.

## Not done / follow-ups (one line each)

- `a` (grid -> list) still draws the 320 stale rows as a list for one frame before the 64-row reply (4.8 ms, under the gate).
- Whole-view reuse across grid/list stays impossible (search text and scroll differ); markup is rebuilt each stage
  (eval 1.0 + serde 1.2 ms for 42 tiles). A keyed per-tile markup cache in `kit.VirtualGrid` is the next lever, not needed for Gate E.
- Shaped-text/tile raster caches: not added. Measured only 2 rebuilt rasters per flip, also with a 500 ms pause.
- The 500 ms paused lane sits at 8.0 ms median (cold caches); the default lane has 0.5 ms margin.

Temporary duplicates: none (the grid cache is not a second source of truth: stamp-keyed, cleared on activation/stale).
