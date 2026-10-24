# B0081 - Gate always-on memory on unreclaimable memory; remove the code-page release

Status: **Review** (Claude worker, branch `task/b0081-memory-gate-20261024`). Owner decision 2026-10-24, option (b), after
[B0069](0069-code-pages-verify.md) proved that B0065's `code_pages::release()` is a counting change: it removed 15-16 MiB from the shell's
PSS and from `file_mapped` and nothing from memory the kernel holds (cgroup `memory.current`, page cache, `MemAvailable` identical).

## Decision

* **Remove** `crates/bead-wayland/src/code_pages.rs`, the module declaration and the calloop timer in `run_runtime` (`runtime.rs`, which also printed
  `CODE_PAGES_RELEASED`). There was no setting and no test beyond the module's own. Net: 86 + 23 product lines removed, none added.
* **Gate on unreclaimable memory** = `Pss_Anon + Pss_Shmem (SHM) + SwapPss + per-client DRM` of the always-on shell. These are the pages the kernel
  cannot drop. PSS, its clean file-backed share (`Pss_File`, program text, rodata, fonts) and the old PSS + DRM + SwapPss total stay in every report
  as **information columns**, never gated.
* **Thresholds.** Laptop reference capture (lead only): candidate median per phase <= **10 MiB**; per-sample stop line **9.5 MiB** (the old
  25 / 24.5 pair kept its 0.5 MiB margin); anonymous growth vs control <= 1 MiB and nonzero SwapPss still stop; close samples stay within own
  bar + 5 MiB (now counted in unreclaimable). Other hosts: relative, candidate - control <= **+0.5 MiB** median per phase, as since 2026-10-21, now on
  the unreclaimable sum. Always record the render backend (CPU vs GL) next to any number.

## Kept from B0065 (independently useful)

* `spikes/b0065-smaps.py` (per-mapping smaps attribution and diff, still the way to see which mapping moved) and the attribution table in
  [B0065](0065-mem-filebacked.md): the larger binary's text/rodata is what raised PSS; fonts, libm, SVG assets and libmpv were not the cause.
* B0069's code-size measurements (opt-level / LTO table, section 5): the only real file-backed saving (about 2-3 MiB resident, up to 4 MiB page cache).
  A build-profile unit is separate and out of scope here.
* Deleted: `spikes/b0069-codepages.py` (its job was comparing the release against none). Its raw summaries stay in `evidence/0069`.

## Tooling changes (all in `spikes/`)

| File | Change |
| --- | --- |
| `laptop-reference-capture.py` | `unreclaimable_mib(row)`; headline `candidate_median_mib` / `control_median_mib` / pair values / close checks are unreclaimable; gate 10 MiB, stop 9.5 (`UNRECLAIMABLE_GATE_MIB`, `UNRECLAIMABLE_STOP_MIB`). New info fields: `*_pss_total_median_mib` (PSS + DRM + SwapPss), `*_clean_file_median_mib`, `pss_delta_mib`, `information_columns`. Manifest, provenance, backend, geometry, isolation and swap gates unchanged. |
| `dashboard-residency.py` | `unreclaimable_total()`; absolute lane rails 24.5 -> 9.5 stop, 25 -> 10; `summarize_batch_phase` judges unreclaimable (relative +0.5 and anon +1 unchanged) and reports PSS medians; the 25-27 MiB "one-time file-backed allowance" is removed (it excused PSS growth that is no longer gated). |
| `dashboard-batch-summary.py` | `--allow-27-compositor` and the 27 MiB report flag removed. |
| `launcher-proof.py`, `bar-matrix.py` | reference stop/ceiling use unreclaimable (9.5 / 10); every idle row gains `unreclaimable_mib`. |
| `b0081-gate.py` (new) | desktop lane used below: per-phase unreclaimable / PSS / clean-file and open latency, alternating arms. |
| tests | `test_laptop_reference_capture.py` (headline is unreclaimable, 10 / 9.5 gate and stop, PSS informational), `test_dashboard_capture_windows.py` (rails, batch median, relative host, summary rewritten for the new gate). 109 tests pass. |

Docs updated: `docs/specs/README.md` (decision note, queue row, B0065/B0069 marked superseded), `AGENTS.md` and `docs/ROADMAP.md` (memory rule
amended; older "PSS+DRM+SwapPss <= 25 / 24.5" text reads as the unreclaimable sum), ADR 0004/0005, and the laptop capture runbook. Dated `STATUS.md`
and spec history entries keep their original numbers on purpose.

## Desktop measurement (control = origin/main `a150afe`, candidate = this branch `fc4b241`)

Desktop (12 cores, 15 GiB), private nested session (headless labwc > niri > `nacre-shell --bar --core-views --renderer cpu --panel-renderer cpu`, no
daemon), **CPU renderer (tiny-skia), DRM = 0**, swap used 13 MiB (< 64 MiB) before and after, `flock desktop-heavy.lock`, `nice -n 10`. Release builds
`--locked`, rustc 1.99.0, one dev-2 Arch container (control 29,796,264 bytes, candidate 29,786,384). Alternating arms, each phase is the median of
5 `smaps_rollup` samples (idle 20 s after start; "open" 3 s after the open; "closed" 12 s after the close). The 12 pairs are pooled from three runs
(3, 5 and 4 pairs, `evidence/0081/b0081-run{1,2,3}.json`); medians in MiB:

| Phase | unreclaimable ctl / cand / delta | PSS ctl / cand / delta | clean file ctl / cand / delta | gate |
| --- | --- | --- | --- | --- |
| idle | 5.95 / 5.72 / **-0.23** | 11.46 / 27.44 / +15.98 | 5.49 / 21.76 / +16.27 | pass |
| launcher open | 14.18 / 14.14 / **-0.04** | 31.49 / 36.40 / +4.91 | 18.04 / 22.23 / +4.19 | pass |
| launcher closed | 6.75 / 6.20 / **-0.54** | 9.50 / 27.11 / +17.61 | 2.65 / 20.92 / +18.27 | pass |
| dashboard open | 14.42 / 14.04 / **-0.38** | 32.86 / 36.47 / +3.61 | 18.79 / 22.45 / +3.66 | pass |
| dashboard closed | 7.01 / 7.38 / **+0.37** | 11.51 / 28.89 / +17.38 | 4.46 / 21.54 / +17.08 | pass (see below) |

Latency (open IPC to first `PANEL_PRESENTED`, medians over the 12 pairs): launcher 11.76 vs 11.74 ms (-0.02), dashboard 15.56 vs 15.58 ms (+0.02): no change.

Reading it honestly:

* **Expected result confirmed.** Unreclaimable memory is unchanged within +-0.5 MiB (it is slightly lower in four phases) and PSS is 16-18 MiB higher
  when closed, exactly the counting effect B0069 measured. The shell's real always-on cost is **5.7-6.2 MiB** unreclaimable at idle/closed on this
  host (CPU backend), under the 10 MiB laptop cap with 4 MiB to spare; with a panel open it is about 14 MiB, so the cap applies to settled samples only,
  as it always did. Closed PSS (27 MiB) is above the old 25 MiB PSS cap by design; it is no longer a gate.
* **Transient after a dashboard close.** At 12 s after the dashboard closes the candidate showed +1.2 to +3.4 MiB anonymous in 5 of 12 runs (8.2, 8.2,
  8.2, 8.2, 9.8, 9.8) and the control in 1 of 12 (10.0); the first 3-pair run alone read +1.14 MiB and would have failed the 0.5 MiB gate, the later
  9 pairs read -0.56 and +0.29. About a minute after the close (phase added in run 3) the candidate was 6.4-6.7 MiB in all 4 runs and the control
  6.7-10.1 (one spike). So it is a decaying post-close allocation that was seen in both arms, more often in the candidate; the laptop capture settles
  60 s, where it is not seen. Not diagnosed further (out of scope); it may be timing, or the periodic 3 s smaps read of the old code incidentally
  touching the allocator. Flagged for the lead.
* Not measured: GL backend, the laptop, the daemon-linked session.

## Checks

* Full Bead CI replicated on dev-2 in `ghcr.io/archlinux/archlinux:latest` (packages as the Ubuntu job plus ttf-dejavu, ttf-ibm-plex, swaybg,
  wlr-randr, git; sway file capability removed; rustc 1.99.0, `-j4`, containers `b0081-ci*`, all removed): `cargo fmt --check`, clippy `-D warnings`,
  service-boundary check, `cargo test --workspace` (+ trace `memory_policy`), feature configurations, facade, Luau analysis, spikes unit tests and
  `py_compile`, packaging tests, services check, tray private-bus test, `cargo deny` (root and framework crates) all pass. The motion-wallpaper
  smoke failed once on `crop_keeps_centre` (video-crop check, unrelated to this change) and passed 3 of 3 on the candidate and 3 of 3 on origin/main
  when repeated; treat it as flaky on this Arch mpv/ffmpeg.
* `python3 -m unittest discover -s spikes/tests`: 109 tests pass.

Temporary duplicates: none added. `spikes/b0081-gate.py` is a measurement lane; delete it 14 days after the first laptop capture under this gate is
accepted.
