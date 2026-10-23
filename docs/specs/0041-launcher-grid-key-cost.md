# B0041 — Launcher grid <-> list key cost and first-key compaction timing

Status: **Review** (Claude worker, branch `task/b0041-grid-virtualize-20261023`). Follows B0040's finding that keys flipping
the launcher between the app grid and the result list cost ~10.9 ms UI CPU (Gate E: <= 8 ms) and that the first key pays
2.4 ms more when the idle SHM pool compaction fired before it. Consistent with ADR 0005 (one process; memory judged by the
relative gate). No new ADR.

## Findings that change the premise

- **The grid is already virtualised.** `kit.VirtualGrid` (B0017/feature 9) materialises only the viewport rows plus one row
  above and below; the other tiles never reach Luau. B0040's "all 320 tiles rebuilt" was wrong: at the 1920 px logical
  fixture the grid is 7 columns x 7 rows = 49 tiles (list: 9 rows). `markup_serde` is 1.3 ms for those 49 tiles, not for 320.
  The model (320 apps) is only key-checked and counted in Rust.
- **Keeping the grid view alive across grid -> list -> grid does not hit.** Tried: a second `Cached`/markup slot in
  `bead-ui` so the previous view is reused when its reads are unchanged. Zero hits in 165 builds: the grid reads 320 apps
  with none selected, the list reads 64 results with row 0 selected, and a key produces two commits (key applied with the
  old model, then the query reply), so the two states are never byte-equal. Reverted; no code kept (expiry n/a).
- One key = two commits (`update=N` key, `N+1` reply). The harness (and Gate E) times the first only.

## Changes (product +23 / -5 lines in 2 files, plus a 45-line test)

1. `bead-wayland/src/rendering.rs`: `SHM_COMPACT_FOCUSED_IDLE = 5 s` (constant beside `SHM_COMPACT_IDLE = 250 ms`). A surface
   whose keyboard interactivity is on (open launcher, any interactive panel) compacts its SHM pool only after 5 s without a
   frame callback; other surfaces keep 250 ms. Hiding destroys the surface and its pool, so close is unchanged. One constant:
   set it to 250 ms to restore the old behaviour (owner veto).
2. `bead-ui/assets/kit.luau` `VirtualGrid`: at the top of the model there is no row above to overscan, so the window is one
   row smaller (49 -> 42 grid tiles, 9 -> 8 list rows at scroll 0); never larger than before elsewhere. Visuals unchanged
   (the dropped row is off-screen); no golden changed (CI below).
3. Test `grid_list_flip_builds_only_the_window_and_returns_the_same_tiles` (`markup_tests.rs`): window size at the top (a
   100 px claimed viewport in a 240 px scroll area, so every built row is countable; fails on the old window), mid-scroll
   bound, and grid -> list -> grid returns identical tile ids.

## Numbers

Harness: `spikes/typing-proof.py` (320 fixture apps, CPU renderer, 2880x1800 @1.5, 60 keys, clocks only), headless sway in
an Arch container on dev-2 (`--cpus=8 --memory=12g`), pixman. Control `b93100b`, candidate = this change, builds from the
same toolchain; 5 alternating runs (default lane), 3 (others). UI CPU ms, median of run medians.

| Lane / key | control | candidate | gate |
| --- | ---: | ---: | --- |
| default p50 (grid <-> list) | 10.99 | 10.31 | <= 8 (open) |
| default p99 CPU | 13.46 | 13.22 | |
| default presented p99 | 9.82 | 9.59 | <= 16.7 ok |
| `a` key (grid -> list) | 6.10 | 5.86 | |
| BackSpace key (list -> grid) | 11.87 | 11.40 | <= 8 (open) |
| first key | 10.44 | 7.71 (runs 7.35-8.99) | <= 8 (4 of 5 runs <= 8.2) |
| steady list -> list p50 | 1.17 | 1.13 | ok |
| changed-results list p50 | 1.29 | 1.25 | ok |
| `SHM_POOL_COMPACT` in a 60-key run | 2 | 0 | |

Window trim alone is -0.6 ms p50 (-5%); it cannot reach 8 ms on the BackSpace key: per-stage (control, ms) markup 2.7,
compose 2.9, view_ir 0.8, bake 0.8, layout 0.5, rest ~2.6, all scaling with the 42-49 tiles that must be drawn. Reaching
<= 8 ms there needs incremental scene/paint reuse below markup, a separate unit.

Memory (launcher-proof, CPU renderer, headless sway, 5 control/candidate pairs, PSS+DRM MiB, median over runs; relative gate
<= +0.5 MiB):

| Phase | control | candidate | delta |
| --- | ---: | ---: | ---: |
| launcher closed idle (bar) | 24.5 | 24.6 | +0.1 (identical code path) |
| after close, settled (cycles 1/2/3) | 26.15/26.22/26.29 | 26.13/26.21/26.35 | -0.0/-0.0/+0.1 pass |
| open, settled >= 12 s idle (typing proof, 3 pairs) | 42.7 | 41.3 | -1.4 pass |
| **open, first 1 s window after open (cycles 1/2/3)** | 39.8/37.9/38.0 | 44.0/47.7/47.7 | **+4.2/+9.8/+9.8** |

The last row is the price: spare frame buffers (shmem 3.7 -> 13.5 MiB) stay resident for up to 5 s after the last frame
while the launcher is open, then compact as before. Anon memory is unchanged. The owner can trade: first key +2.4 ms CPU
(once, if the user pauses > 250 ms after open) against up to ~10 MiB for the first 5 s of an open launcher. Not applied to
the closed bar, so the 25 MiB laptop cap (closed) is unaffected; the laptop reference capture is the lead's.

## Not done / follow-ups (one line each)

- BackSpace -> grid (11.4 ms) and `a` (5.9 ms): needs scene/paint reuse below markup; not a virtualisation problem.
- With the old model still in place, the first commit of a BackSpace draws the grid from 64 results (row 0 highlighted)
  until the reply arrives (existing behaviour, one frame; not changed).
- Desktop niri nested run skipped (headless sway only). No live switch.

No temporary duplicates introduced (nothing to expire).
