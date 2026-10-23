# B0040 — Steady typing UI CPU: bisect result (no steady regression)

Status: **Review** (Claude worker, branch `task/b0040-typing-regression-20261023`). Investigates the B0039 finding
"steady UI CPU 13.7 ms p50 vs 4.9 ms recorded by B0019, `markup` 5.3 ms vs 1.1 ms". **No product change.** The
finding compared two different workloads; a bisect of the right one shows no steady-typing regression since
B0019. A real, smaller first-key cost remains and is named below.

## Result

Same harness (`spikes/typing-proof.py`, 320 fixture apps, CPU renderer, 2880x1800 at scale 1.5, headless sway in an
Arch container on dev-2, 60 keys, clocks only), each commit built with its own `shell/` tree and `spikes/`. Control
`a8b4b6e` (B0019 landing) against current `d257933`; medians of two to four runs, UI CPU ms:

| Lane (env) | What a key does | `a8b4b6e` | `d257933` |
| --- | --- | ---: | ---: |
| default (B0039's lane) | `a` / BackSpace alternate: grid -> list -> grid, 320 tiles rebuilt every other key | p50 10.8-10.9 | p50 10.9-11.0 |
| `BEAD_TYPING_STEADY_LIST=1` | first key, then `p` / BackSpace: list -> list | p50 1.2 | p50 1.2 |
| `+ BEAD_TYPING_CHANGED_RESULTS=1` | same, with a different 160-row result set per key | p50 1.3, p99 2.3 | p50 1.3, p99 2.1-2.3 |

- B0039's 13.7 ms is the grid <-> list lane (10.8-11 ms here; niri is slower than sway). B0019's 4.9 ms and B0003's
  3.2 ms are the list -> list lane against the real engine (`BEAD_HARDENING=menu-engine`). The two were never
  comparable. Per key type in the stage run, identical at both commits: the BackSpace (list -> grid) key costs 12.9
  ms with `markup` 5.6 ms (`markup_eval` 2.1, `markup_serde` 2.7: 320 tiles), the `a` (grid -> list) key 6.9 ms with
  `markup` 2.3 ms.
- Steady list -> list typing is 1.2-1.3 ms in the fixture lane on both commits (the real-engine lane adds the
  RPC/codec work; it needs the Nacre helper and was not rerun). The suspected B0032 `tr()` calls, B0035/B0036
  prefs, B0037/B0038 rail and `menu.groups`, B0025 sessions, B0026 palette cache, B0028 hide and B0020/B0030 notices
  add nothing measurable to a typed key. `menu.groups` and `menu.preferences` are fetched only when the source stamp
  changes, not per key (`launcher.rs`, `groups_task`).
- Therefore there is no commit to fix for steady CPU, and no code change was made. The Gate E steady target (<= 8 ms
  per key) is met by list -> list typing; only the grid <-> list transition keys exceed it, by design of the current
  320-tile markup (see "Not done").

## The first-key cost that is real

First key (UI CPU, clocks): `a8b4b6e` 8.7, `d257933` 10.4. `grid -> list` costs a full repaint (compose 6.9 vs 1.5 ms)
exactly when the idle SHM pool compaction fired before the key (`SHM_POOL_COMPACT` in the log; `trim_idle_shm` fires
when more than one buffer was painted, which depends on startup frame timing, so the cost flips between otherwise
unrelated commits: it appeared at `ff616d0`, a formatting commit, and not at `c334a5b`). B0039's buffer adoption
removes the full repaint but not the rest:

| `d257933` experiment (first key UI CPU, 3-5 runs) | ms |
| --- | ---: |
| as built (compaction fires) | 10.0-10.6 |
| compaction disabled | **7.7-8.0** |
| compaction kept, no `malloc_trim` after its repaint | 9.9-11.8 |
| compaction kept, `MADV_POPULATE_WRITE` on the adopted buffer | 9.9-10.5 (no gain, not kept) |
| `retain_scales` call removed from the worker | no change |

So the 2.4 ms remaining over the no-compaction first key is the price of retiring and rebuilding the pool; it is not
the buffer copy's page faults, `malloc_trim` or `retain_scales`. Disabling compaction would meet the 8 ms first-key
gate but retains spare frame pages (B0025's idle-memory work); that is a memory-versus-latency decision for the
owner, not taken here.

## Method notes (for the next bisect)

- Many commits between `ecd8e2a` and `8b1f7ba` do not compile (`E0507` in `nacre-shell/src/main.rs`, fixed by the
  one-line `8b1f7ba`); a build script that ignores cargo's exit status silently measures the previous binary. Check
  the build status, then compare binary hashes.
- First-key CPU is bimodal on the compaction condition above. Log `SHM_POOL_COMPACT` counts next to every first-key
  number; compare only equal counts.
- Always name the lane: the output's `workload` field distinguishes alternating grid-list from populated-list runs.
- Host: dev-2 Arch container (`--cpus=8 --memory=12g`), headless sway (`WLR_BACKENDS=headless`, pixman, output
  `2880x1800` scale 1.5, `setcap -r /usr/bin/sway` for the container), `wtype`, shared release target dir.

## Not done (one line each)

- Cheaper grid <-> list transition: markup rebuilds and serialises all 320 tiles (`markup_serde` 2.7 ms); virtualising
  the grid would be its own unit and is the only path to <= 8 ms on those keys.
- Real-engine lane at `d257933`: needs the Nacre helper binary; the fixture lane equals it except for RPC work.
- No regression guard added: there is no regression, and a timing assertion needs a compositor (not CI-able).

No temporary duplicates were introduced (nothing to expire).
