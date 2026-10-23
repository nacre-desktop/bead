# B0039 — Launcher first-key latency

Status: **Review** (Claude worker, branch `task/b0039-first-key-latency-20261023`). Follows
[B0003](0003-launcher-typing-investigation.md) and [B0019](0019-launcher-parity.md), which measured the first typed
key at 25-29 ms presented and 9-15 ms UI CPU against steady keys under 8 ms. Gate E targets: <= 8 ms UI CPU per key,
presented p99 <= 16.7 ms.

## Cause (profiled, not guessed)

Method: `spikes/typing-proof.py` (320 fixture apps, serial keys, `a`/BackSpace alternating, CPU renderer, 2880x1800 at
scale 1.5), stage spans on (`BEAD_PROFILE_INPUT=1`) for the breakdown and clocks only for the headline numbers. The
first key is the grid-to-list transition after the launcher has been quiet. Comparing it with the next list key
(`a`, same layout) isolates the first-key-only work:

| First key minus a steady `a` key (CPU, stage run) | ms | Cause |
| --- | ---: | --- |
| `compose` (UI thread) | +3.3 | **Full-surface repaint into a never-painted spare buffer.** 250 ms after typing stops, `trim_idle_shm` retires the SHM pool and repaints one frame into a fresh pool, so its spare buffers are unpainted with `pending = [full]`. The next key rotates onto one of them and paints 1.7 Mpx (`paint_clear` 1.5 ms of first-touch faults, `paint_rounded` 5.7 vs 1.9 ms) instead of the ~0.9 Mpx of real damage. |
| `font_select` (worker) | +0.75 | The first query adds characters the opening grid did not need, so the selection cache misses and Fontconfig runs on the input path. |
| `bake` incl. `shape` (worker) | +1.7 | First shaping and rasterisation of the 30 visible list rows (new strings, new sizes). Inherent per new result text, not first-key-only (see below). |

Priming the same run with an extra `a`/BackSpace pair removed the `bake`/`font_select`/`shape` excess but not the
`compose` excess, which is what showed that the repaint comes from the idle pool compaction and not from cold caches.

## Changes (3 files, +48/-4 product lines)

- `crates/bead-wayland/src/rendering.rs`: when the buffer chosen for a frame has never been painted and another buffer
  has, copy the painted buffer's pixels (`raw_data_mut`, because the compositor still holds it and `canvas()` refuses
  it) and adopt its pending repairs, instead of repainting the whole surface. Pixels equal source + pending damage by
  the existing invariant, so only the real damage is rasterised. Memory is unchanged: the same pages are touched
  either way.
- `crates/nacre-shell/src/font_sources.rs`, `reload.rs`: the launcher's `Selector` also selects for printable ASCII
  (`Selector::with_ascii`), so the opening selection already covers a typed Latin query. Same two font files; the
  Fontconfig cost moves from the first key to launcher open (4.1 ms to 4.4 ms, off the input path). Other sessions
  (bar, panels) are untouched.

Not done (one line each): pre-warming glyph rasters for the list sizes needs the list text sizes outside the Luau
markup (a duplicated constant) and the 1.7 ms is mostly per-string shaping that every new query pays anyway; rounded
painting needs no change now that the repaint is no longer full; icon decode and Luau first-call cost were not
visible in the first-key stages (`markup`, `markup_setup`, `view_ir` equal to steady).

## Results (5 alternating base/candidate pairs per host, 60 keys, clocks only, medians [min-max])

Same harness, same host, same flags; control = `main` at `bb5787c`, candidate = this branch. Render backend: **CPU**.

| Host | Metric | Control | Candidate |
| --- | --- | ---: | ---: |
| Desktop, niri nested (private session) | first key UI CPU ms | 18.06 [17.96-18.54] | **12.77** [12.55-12.93] |
| | first key presented ms | 16.26 [16.23-16.72] | **10.92** [10.73-11.10] |
| | steady UI CPU p50 / p99 ms | 13.70 / 15.46 | 13.41 / 15.59 |
| | steady presented p99 ms | 11.60 | 11.49 |
| Container, headless sway (dev-2) | first key UI CPU ms | 15.20 [14.93-17.86] | **10.83** [10.21-11.14] |
| | first key presented ms | 12.85 [12.59-14.68] | **8.37** [7.85-8.65] |
| | steady UI CPU p50 / p99 ms | 11.06 / 13.11 | 10.84 / 13.01 |
| | steady presented p99 ms | 9.60 | 9.44 |

First key: -5.3 ms CPU and -5.3 ms presented on niri (-29 % / -33 %), -4.4 / -4.5 ms on sway. Steady keys are unchanged
within noise. Gate E: first-key presented now meets 16.7 ms on both hosts; **first-key UI CPU still misses 8 ms**
(12.8 / 10.8). It also misses on steady keys in this build (above), so this unit does not claim Gate E acceptance.

## Correctness and memory

- Pixels: `grim` screenshots taken right after 1, 2, 3, 4 and 7 keys (no compaction in between, verified in the log)
  are byte-identical between control and candidate (headless sway, same hashes), so copy plus pending repair
  reproduces the full repaint.
- Memory, relative gate (candidate - control, same host and backend, CPU, PSS MiB medians over 5 pairs):
  niri post-key -0.030 (anon -0.117, shmem 0), idle after 15 s -0.030; sway post-key +0.157 (anon +0.035, shmem 0),
  idle +0.157. All within +0.5 MiB, no anon growth over 1 MiB. SwapPss 0. The absolute 25 MiB laptop cap is the lead's
  reference capture and was not run. DRM memory is empty for the CPU renderer.
- ADR 0005: no new process or session; the launcher stays a session of the one UI process. No temporary duplicates were
  introduced (nothing to expire).

## Findings outside scope (report-only)

- Steady-state cost has grown since B0019: control `main` shows steady UI CPU p50 13.7 ms on the same desktop niri
  harness (B0019 recorded 4.9, p99 5.35) and `markup` steady CPU 5.3 ms (`markup_eval` 2.0 + `markup_serde` 2.6 for the
  320-row model) against 1.1 ms in B0003. Not bisected; the B0019 binary no longer loads the current shell bundle, so a
  direct re-run was impossible. Likely candidates are the row model growth from B0032 to B0038.
- The harness's steady keys alternate between two fixed result sets, so text and glyph caches stay warm; real queries
  pay `bake`/`shape` on every new result set.

## Tests and CI

`cargo fmt --check`, warnings-denied clippy, workspace tests and the rest of the `ci.yml` job were run in an Arch
container on dev-2; results are in the worker report. No Rust unit test was added: the buffer path needs a live SHM
pool, so the pixel comparison above is the evidence.
