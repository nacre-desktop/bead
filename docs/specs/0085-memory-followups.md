# B0085 - Memory follow-ups: smaller release profile; the dashboard-close transient

Status: **Review** (Claude worker, branch `task/b0085-memory-followups-20261024`). Part 1 landed as a release profile; Part 2 is diagnosed, no fix
is justified by the data.

## Part 1 - release profile

`Cargo.toml` `[profile.release]` now has `lto = "fat"`; dependencies are `opt-level = "s"` (`[profile.release.package."*"]`, which does not
touch workspace crates) with `tiny-skia`, `tiny-skia-path`, `skrifa`, `read-fonts`, `harfrust`, `zeno`, `taffy`, `usvg` and `cosmic-text` back at 3.
All workspace crates (`bead-*`, `nacre-shell`) stay at 3. Release only: CI, `cargo test` and dev builds are unchanged. `packaging/arch/PKGBUILD` and
`refresh-bead-preview` both run `cargo build --release`, so both pick it up.

Variants (table below, built from the `295ea41` tree) built from the same source (Rust 1.99.x Arch container, `--cpus=12`, `-j4`) and measured on the desktop against the same-flag control
(nested niri, CPU backend, no daemon, `flock desktop-heavy.lock`, 3 alternating pairs, 60 keys per arm; `spikes/b0085-gate.py`, `b0081-gate.py`).
First key = UI CPU ms from input receipt to presentation (B0075/B0080 clocks). The Home lane is a bar launcher with a 12-favourite/8-recent
fixture and a 64-of-320 result; it is an approximation of the B0080 lane (that harness is not in the repo), the standalone lane is `typing-proof.py`.

| Variant (fat LTO unless noted) | binary MiB | build s | first key standalone / Home (ctl 5.4-5.6 / 5.8-5.9) | later keys | launcher / dashboard open ms (ctl) | clean file MiB | unreclaimable |
| --- | ---: | ---: | --- | --- | --- | ---: | --- |
| control: opt 3, no LTO | 28.5 | 205 | - | - | 11.8 / 15.1-15.3 | - | - |
| opt 3 + fat LTO | 25.0 | 274 | 5.24 / 5.67 (-0.3) | unchanged | 11.6 / +0.17 | -0.6 to -1.0 | -0.1 |
| opt s everywhere | 18.9 | 202 | **7.25 / 7.66 (+1.8)** | **+2.5 / +2.6** | not run | not run | not run |
| mix: s, 9 render/text crates at 3 | 19.3 | 212 | 5.81 / 6.24 (+0.35 / +0.48) | +0.6 / +0.7 | 11.6 / **18.2 (+3.1)** | -5.2 | -0.1 |
| mix3: s deps + nacre-shell, services, backends, wallpaper | 20.8 | 269 | 5.65 / 6.26 (+0.11 / +0.33) | +0.5 / +0.3 | 11.7 / **+3.0** | -3.9 | -0.2 |
| **mix2 (shipped): s deps only, hot crates + workspace at 3** | 23.1 | 285 | 5.43 / 5.82 (+0.07 / -0.04) | +0.03 / -0.2 | 11.6 / 15.3 (0.0) | **-2.0** | +0.0 |

The dashboard open path (nacre-shell glue, services) is what `s` slows by 3 ms, which fails "dashboard reveal unchanged"; mix and mix3 save more
(-5.2 / -3.9 MiB clean file) and are the option if the owner trades 3 ms of dashboard open for 2-3 MiB more.

**Final run on current main** (`c7495b6`, control = origin/main release, candidate = shipped profile; control 29,976,416 bytes, candidate 24,394,648;
build 242 s -> 306 s, +26 %, release only). Medians, MiB:

| Phase | unreclaimable ctl / cand / delta | PSS ctl / cand / delta | clean file ctl / cand / delta |
| --- | --- | --- | --- |
| idle | 5.58 / 5.68 / +0.10 | 26.47 / 24.27 / -2.20 | 20.89 / 18.59 / -2.30 |
| launcher open | 14.11 / 14.22 / +0.11 | 35.16 / 33.42 / -1.74 | 21.38 / 19.24 / -2.14 |
| launcher closed | 6.09 / 6.15 / +0.05 | 26.27 / 24.09 / -2.19 | 20.07 / 17.94 / -2.13 |
| dashboard open | 13.94 / 14.08 / +0.14 | 35.56 / 33.16 / -2.40 | 21.62 / 19.03 / -2.60 |
| dashboard closed (12 s) | 6.57 / 6.48 / -0.09 | 27.27 / 24.69 / -2.57 | 20.70 / 18.10 / -2.60 |
| dashboard settled | 6.74 / 6.51 / -0.24 | 27.45 / 24.72 / -2.73 | 20.70 / 18.10 / -2.60 |

Latency, 3 pairs: launcher open 11.87 -> 11.64 ms, dashboard open 15.65 -> 15.71 ms (+0.06); first key standalone 5.59 -> 5.38, Home 5.88 -> 5.88 ms
(gate 8 ms, +2 ms open). Every memory gate passes (<= +0.5 MiB). CPU backend; the saving is clean file-backed memory (reported, not gated) and
matches B0069's 2-3 MiB.

## Part 2 - the dashboard-close transient

B0081 saw +1.2 to +3.4 MiB anon 12 s after a dashboard close. Time series (`spikes/b0085-transient.py`, 1 s samples for 60 s after the close,
control = main, 6 to 11 runs per arm; unreclaimable minus the pre-open idle value):

* Right after the close (+3 s) the shell sits **+0.4 to +0.7 MiB** above idle and stays there for the whole 60 s: the panel worker's thread arena keeps
  about 0.45 MiB (new `[anon]` mapping next to the 2.5 MiB bar arena) plus 50 KiB of heap. The existing release (2.2 s timer, trim at 2.2 and 2.3 s,
  SHM compaction trim) already did all it can; the value is flat from +3 s, well inside the 5 s goal, but about 0.6 MiB is retained, not zero.
* A separate **+1.6 to +1.7 MiB step in the 2.5 MiB bar arena** appears at an arbitrary time (seen at +36 s, +50 s, +57 s after a close) in about a third
  of the runs, and also with **no panel ever opened** (idle-only run: +1.76 MiB at 62 s). It is released again at a later SHM compaction (minute tick).
  It is not tied to the close (B0081's "5 of 12 at 12 s" came from short sampling windows catching it), not to the audio service's 1.6 s PipeWire
  retry loop (the step is equally frequent with a private PipeWire server: 2 of 6 runs with, 2 of 6 without), and not to the clock tick.
* A 10 s `malloc_trim(0)` timer (tried and measured: candidate runs still show the step in 2 of 6 runs, up to +2.4 MiB) does **not** remove it. The step
  is therefore not free memory held back by the allocator; it is live data (a cache growth in the bar worker's thread) that is dropped later. Not
  fixed: the culprit needs allocation tracing (heaptrack on the nested shell) and the cost of any blind fix (periodic wakeups) is real. The timer was
  reverted; nothing in product code changed for Part 2.
* Consequence for gates: the 12 s "closed" sample can read +1.5 to +3 MiB for a run for reasons independent of the panel and of the build profile
  (the control showed it in 2 of 6 runs, the candidate in 2 of 6, and in mix2's first 3-pair run the dashboard-closed delta read +1.46). Judge dashboard-closed
  by the 3-pair median and the settled phase, as B0081 did.

Follow-up proposal (lead's call): trace the bar-worker allocations with heaptrack to name the +1.6 MiB cache; separately, trimming the 0.45 MiB panel-worker
arena would need that worker to free instead of keep its caches at retirement.

## Tooling

`spikes/b0085-gate.py` (first-key standalone and Home lanes, alternating arms) and `spikes/b0085-transient.py` (time series; `idle` mode; optional private
PipeWire). Delete both 14 days after the next profile or memory unit is accepted. No temporary duplicates in product code.

## Checks

Full Bead CI replicated on prod in `ghcr.io/archlinux/archlinux:latest` (packages as the Ubuntu job plus ttf-dejavu, ttf-ibm-plex, swaybg, wlr-randr; sway
capability removed; `-j4`; containers `b0085-*`, removed): see the final report for results. CI runs no release build, so the profile is exercised by the
release builds above only.
