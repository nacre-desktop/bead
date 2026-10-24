# B0069 - Verify the B0065 code-page release: what it changes, what it costs

Status: **Superseded by [B0081](0081-memory-gate-unreclaimable.md)** (the `code_pages` release described here was removed; its result stays as history). Was: **Review** (Claude worker, branch `task/b0069-codepages-verify-20261023`). No product code changed. Question from the lead: B0065's
`code_pages::release()` (`madvise(MADV_DONTNEED)` on clean file mappings every 3 s while closed-idle) took PSS from about 24 to 10-12 MiB, but clean
file pages stay in the page cache and are reclaimable anyway, so is it a counting change, and does it cost latency?

**Answer: it is a counting change with no measurable latency cost.** It removes 15-16 MiB from the shell's PSS and from the cgroup's `file_mapped`,
and removes nothing from memory the kernel has charged or can free (cgroup `memory.current`, the page cache of the binary, host `MemAvailable` are the
same with and without). Launcher/dashboard/key latency are within noise (+0.1 ms); idle CPU is +0.05 % of a core. **Recommendation: keep it as is**,
label it honestly (below), and treat code-size reduction as a separate, smaller real saving (2-3 MiB resident, proposals with numbers below).

## Method

Desktop (12 cores, 15 GiB), private nested session as B0065 (`b0047-frame.py` `Arm`: dbus + headless labwc > niri > `nacre-shell --bar --core-views
--renderer cpu --panel-renderer cpu`, no daemon), CPU renderer (tiny-skia) throughout, swap used 0 before every arm, `flock desktop-heavy.lock`, `nice -n 10`.
Harness: `spikes/b0069-codepages.py` (`latency`, `idle`, `account`, `pressure`); raw summaries `evidence/0069/b0069-code-pages.json`.

* **cp** = HEAD `8408786` as is. **nocp** = the same tree with only the release timer block removed from `bead-wayland/src/runtime.rs` (binaries 29,391,488
  and 29,381,584 bytes), built in one dev-2 Arch container (rustc 1.99.0, `cargo build -p nacre-shell --release --locked`). Same flags, same output layout.
* Arms alternate (cp,nocp / nocp,cp ...). Each arm waits 12 s closed-idle (four releases in cp) before each interaction; two cycles per arm.
* Latency: IPC send to the first `PANEL_PRESENTED` (CLOCK_MONOTONIC, includes the IPC client spawn, equal in both arms); keys by `wtype` with
  `BEAD_PROFILE_INPUT=clocks` (`INPUT_RECEIVED` to `UI_PRESENTED`, UI CPU = process CPU between them); the launcher has the B0039/B0044 fixture (320 apps, `a`
  then BackSpace). Faults and process CPU are `/proc/PID/stat` deltas over the interaction plus 2 s settle (CPU in 10 ms ticks).
* The B0039/B0044 typing lane itself (`typing-proof.py`) runs the launcher as a window, and the release only runs with no window/overlay/panel, so that lane
  cannot see this change at all. The lane here is the same fixture and keys through the bar's launcher panel, opened after idle, which is where the release acts.

## 1. Latency after idle (5 alternating pairs, 2 cycles each, n = 10 per arm, medians)

| Interaction (after >= 12 s idle) | cp | nocp | delta | gate |
| --- | ---: | ---: | ---: | --- |
| launcher open -> first presented frame, ms | 12.89 | 12.77 | +0.12 | within +2 ms: ok |
| first key `a` presented, ms | 9.81 | 9.79 | +0.02 | no regression |
| first key `a` UI CPU, ms | 9.34 | 9.34 | 0.00 | unchanged (see note) |
| BackSpace presented / UI CPU, ms | 11.52 / 8.36 | 11.51 / 8.35 | +0.01 / +0.01 | no regression |
| dashboard open -> first presented frame, ms | 12.54 | 12.46 | +0.08 | no regression |
| minor faults: launcher open / dashboard open | 5210 / 5324 | 5133 / 5279 | +1.5 % / +0.8 % | - |
| minor faults: key `a` / BackSpace | 940 / 285 | 916 / 290 | +2.6 % / -1.7 % | - |
| major faults (all interactions) | 0 | 0 | 0 | - |

Per-sample spread is 12.6-13.1 ms (cp) and 12.6-13.6 ms (nocp) for launcher open. Note: the first-key UI CPU in this lane (bar panel, fixture, CPU renderer)
is 9.3 ms in **both** arms, above the 8 ms Gate E target; that is the existing state of this lane (B0039/B0044 measured 6.2-7.8 ms in the window lane) and
not caused by the release. After the memory pressure below the numbers are the same (launcher 12.55 vs 12.43 ms, first key UI CPU 2.1-2.2 ms in both,
no major faults).

## 2. CPU cost (2 alternating pairs, 300 s closed-idle each)

| | cp | nocp |
| --- | ---: | ---: |
| process CPU per 300 s, ms | 410, 410 | 270, 260 |
| as share of one core | 0.137 % | 0.088 % |
| minor faults per 300 s | 26,632 / 26,480 | 14,646 / 14,813 |
| context switches per 300 s | 342 / 340 | 335 / 333 |
| PSS at the end, MiB | 13.4 / 13.3 | 27.1 / 27.5 |

The release costs about 145 ms CPU per 5 minutes (+0.05 % of a core, no extra wakeups, the loop already ticks each second) and re-faults about 120 pages per
release. `madvise` calls: 63 qualifying mappings (35.6 MiB of address space) per release, one release per 3 s = about 6,300 calls per 5 minutes. This is computed
with the same filter as `code_pages.rs` on the live `/proc/PID/smaps`; the desktop has no `strace`/`perf`, and the product code was deliberately not changed
to add a counter.

## 3. System-level truth (cgroup under memory pressure)

The whole nested session (labwc, niri, shell, dbus, hogs) runs in one transient scope (`systemd-run --user --scope -p MemoryHigh=400M -p MemoryMax=450M -p
MemorySwapMax=0`), so reclaim is confined to that cgroup; the desktop host is never pressured. The binary is copied inside the scope so its page cache is charged
to (and reclaimable from) the cgroup. Stages: warm the launcher once, idle, **file pressure** (read a 512 MiB file 3x, cgroup goes to its limit), idle, **anon
pressure** (a hog takes the cgroup to 324 MiB anonymous, file cache squeezed from 270 to 47 MiB), reopen the launcher and press a key under that pressure. 3
alternating pairs, medians, MiB:

| | cp idle | nocp idle | cp anon-pressure | nocp anon-pressure |
| --- | ---: | ---: | ---: | ---: |
| shell PSS | 10.9 | 27.3 | 9.6 | 27.7 |
| cgroup `file_mapped` | 19.5 | 35.0 | 18.7 | 35.0 |
| cgroup `memory.current` | 174.5 | 176.4 | 382.2 | 381.0 |
| cgroup `file` (page cache charged) | 47.8 | 46.9 | 48.1 | 46.9 |
| binary resident in page cache (`fincore`) | 25.1 | 25.1 | 25.1 | 25.1 |
| host `MemAvailable` | 12,085 | 12,091 | 11,889 | 11,878 |
| host `Mapped` | 337.8 | 352.9 | 336.2 | 352.8 |

* Only the **mapped** and **PSS** columns move (-15.7 MiB). Charged memory (`memory.current`), page cache and host `MemAvailable` are equal within noise.
* The kernel never evicted the shell's text in either arm, even with the cache squeezed to 47 MiB, so there were **0 major faults** and the same reopen latency
  (launcher 12.55 vs 12.43 ms under pressure, 12.77 vs 12.40 ms after the file stage; the +0.1 to +0.4 ms is the only visible cost).
* So the release does not make more RAM available; it makes the process look smaller to PSS/RSS/`Mapped` accounting. It would only free memory if the kernel
  actually dropped those pages, and under the strongest pressure we could apply safely it preferred other file pages.

## 4. Honest accounting (always-on shell, PSS MiB, 3 pairs, 20 s settle; "opened" = launcher + key + dashboard opened and closed once, 20 s settle)

| Kind | idle nocp | idle cp | opened nocp | opened cp |
| --- | ---: | ---: | ---: | ---: |
| anon (heap, stacks, written RELRO/`.data` pages) | 5.45 | 5.60 | 6.44 | 6.94 |
| dirty file-backed | 0.00 | 0.00 | 0.00 | 0.00 |
| clean file-backed text + rodata (`r-xp`, `r--p`) | **19.40** | **3.73** | **20.38** | **1.71** |
| other clean file-backed (fonts, library data) | 1.60 | 1.62 | 0.56 | 0.56 |
| shmem / SHM buffers | 0.21 | 0.21 | 0.21 | 0.21 |
| DRM (fdinfo) | 0 | 0 | 0 | 0 |
| **PSS total** | **26.69** | **11.27** | **27.72** | **9.55** |

Dirty file-backed is zero because written pages of private file mappings are anonymous copies and are counted under anon. The CPU renderer has no DRM client.
The anon delta (+0.15 idle, +0.5 opened) is run-to-run spread of the same size as B0065 saw without the release; it is not a leak and not new. Of the 22.5
MiB of read-only/executable sections loaded, 86 % is mapped in nocp (fault-around and large folios map ahead of use) while a closed shell touches 2-4 MiB.

**What the laptop gate counts:** `laptop-reference-capture.py` takes `Pss + SwapPss + per-client DRM fdinfo` of the shell process (`smaps_rollup`), so it
counts every row above and **does not** count page cache, `Cached`/`Mapped`, RSS, cgroup `memory.current`, or the other Bead processes (`--app`, wallpaper
helper, daemon). Therefore the release counts in full toward the 25 MiB cap, while real unreclaimable RAM (anon + shmem) is about 6-7 MiB either way.

## 5. Code-size reduction, measured (builds only, no product change landed)

Same tree, dev-2 Arch container, `CARGO_PROFILE_RELEASE_*` env overrides (no source change), loaded = SHF_ALLOC sections of the binary:

| Build | loaded MiB (read-only + exec) | `.text` MiB | vs baseline |
| --- | ---: | ---: | ---: |
| baseline (opt-level 3, codegen-units 1, no LTO) | 22.50 | 17.75 | - |
| opt-level 2 | 22.01 | 17.30 | -2 % |
| lto = thin | 22.60 | 18.55 | +0.4 % |
| lto = fat | 20.75 | 17.26 | -8 % |
| opt-level s | 18.10 | 12.44 | -20 % |
| opt-level s + fat LTO | 14.56 | - | -35 % |
| "mix": opt-level s except the render/text crates at 3 (tiny-skia, tiny-skia-path, bead-render, skrifa, read-fonts, harfrust, zeno, taffy, usvg) | 20.91 | - | -7 % |
| **mix + fat LTO** | **18.58** | - | **-17 %** |

`panic = abort` was not tried: `catch_unwind` isolates worker and watcher panics (`bead-wayland/worker.rs`, `bead-ui/watcher.rs`, `nacre_source.rs`).
`strip` only shrinks the file (symbols are not loaded), not PSS.

Measured on the desktop **without** the release (so the cost/benefit of code size alone): "mix + fat LTO" against the baseline, 2 pairs for PSS, 3 pairs for
latency:

| | baseline | mix + fat | delta |
| --- | ---: | ---: | ---: |
| clean text + rodata PSS idle / opened, MiB | 19.50 / 20.50 | 17.22 / 17.52 | -2.3 / -3.0 |
| PSS total idle / opened, MiB | 26.84 / 27.71 | 24.51 / 24.69 | -2.3 / -3.0 |
| launcher open / dashboard open, ms | 12.75 / 12.52 | 12.82 / 12.69 | +0.07 / +0.17 |
| first key presented / UI CPU, ms | 9.82 / 9.38 | 9.68 / 9.20 | -0.14 / -0.18 |

So code-size work buys about 2-3 MiB of PSS (resident share is about 90 % of what is removed) without a latency cost, against 15-16 MiB for the release, and
it shrinks page cache by up to about 4 MiB. Neither changes anonymous memory. Full release builds of the variants took 1.4-3.3 minutes on the 8-CPU container (fat LTO the longest), so build time is not a blocker.

**Separate apps binary:** symbols of the app-only modules in `nacre-shell` (`settings` 0.25, `colors` 0.08, `chat` 0.06, `welcome` 0.03, `app` 0.03 MiB) are about
0.45 MiB of 22.5 MiB loaded (2 %), and apps already run as `--app` processes of the same binary. The rest of the binary is shared framework (zbus/zvariant
1.9, Luau + mlua 1.0, accesskit 0.45, skia/fonts/taffy about 1.5 MiB by symbol size) that a second binary would duplicate. Not worth it: well under 0.5 MiB resident.
(Symbol attribution is approximate: generic instantiations land under `core`/`alloc` at 3.5 MiB.)

## Recommendation

**Keep `code_pages` as is.** It costs nothing measurable (launcher +0.1 ms, first key and dashboard unchanged, no major faults, +0.05 % of a core), it is the
only thing that brings the PSS-based laptop gate (and the relative desktop gate) in range, and the alternatives are weaker or riskier:

* Longer interval / only after long idle: saves a few ms of CPU per minute but widens the window in which a panel's touched pages are counted (B0065 saw single
  samples of 19-21 MiB between releases). Not worth it.
* Remove it: PSS returns to 27-28 MiB on this desktop, above the 25 MiB cap, with no real RAM gained back.
* Replace it with code-size reduction: only 2-3 MiB of PSS (proposal below), not a replacement.

Follow-ups (proposals, none landed; lead decides):

1. **Say what it is.** Gate reports and B0065 wording should state that the 12-14 MiB drop is mapping/PSS accounting of file-backed text, not freed RAM; real
   unreclaimable memory of the always-on shell is anon + shmem (about 6-7 MiB). Keep reporting anon and shmem separately next to PSS.
2. **Build profile (small, measured):** `[profile.release.package."*"] opt-level = "s"` with the nine render/text crates above kept at 3, plus `lto = "fat"`:
   -17 % loaded code, about -2.3 to -3.0 MiB PSS, no latency change in the lane above. Needs the full memory/latency batch on the laptop before adopting, and a
   check of the CPU-renderer p99 on large scenes (not measured here). `lto = "fat"` alone is -8 %.
3. Do not split a separate apps binary and do not try `panic = abort`.

## Checks

No product code changed, so the full remote CI job was not re-run (`spikes/b0069-codepages.py` is a measurement tool, `py_compile` ok, not in the CI list).
Builds: dev-2 container `b0069-ci` (cp/nocp), `b0069-size`, `b0069-size2` (profile variants), all removed afterwards. Desktop runs under the heavy lock, swap used 0 B
before and after every batch (`free -h` logs kept under `~/nacre-lead/b0069/*.free-*.txt`, removed with the directory).

Temporary duplicates: none. B0081 removed `code_pages` and deleted the harness `spikes/b0069-codepages.py` (its raw summaries stay under `evidence/0069`).
