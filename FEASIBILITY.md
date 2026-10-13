# Feasibility

## Phase 0 — research (2026-10-13)

Research recommended a bounded CPU-first experiment, not a full-parity promise.
The final scope was 224 QML files / 27,854 lines / 43 IPC handlers.
Blank-surface results exposed a large first-run Vello cost; every row is retained.
The owner approved ADR 0001, expanded permissive licenses and chose GPL shell adapters.

| Blank renderer | Median PSS / USS MiB | First-run PSS MiB |
| --- | ---: | ---: |
| tiny-skia | 1.88 / 1.83 | 1.86 |
| femtovg/wgpu | 19.88 / 15.46 | 26.79 |
| Vello/wgpu | 32.39 / 27.98 | 177.48 |
| Quickshell | 59.57 / 47.31 | 59.51 |

Three 60-second samples each; zero recorded ticks/SwapPss. QML second run reached
71.85 MiB. Different startup markers were not a speed comparison. Full details:
[research](docs/research/02-rendering.md), [original rows](spikes/results/baseline.json).
~103 minutes to research review, ~640 tooling lines, 0 production lines then.
Verdict: concern / bounded proceed; fonts/effects/reload/full-shell savings unproven.

## Feature 1 — surface skeleton (2026-10-13)

Bead now creates a click-through, non-focusing blank strip on each Wayland output.
The GPL shell runs it in nested Hyprland and niri, sleeps when idle and exits cleanly.
Hyprland passed second-output add/remove/re-add; niri passed scale-change redraws.
Cargo-deny checks the full workspace and permissive-only framework graphs in CI.

| Compositor / client | PSS / USS MiB | Idle CPU | Startup ms |
| --- | ---: | ---: | ---: |
| Hyprland / Bead | 1.06 / 0.94 | <0.017% | 5.00 |
| Hyprland / QML | 71.52 / 56.35 | <0.017% | 156.91 |
| niri / Bead | 1.10 / 1.01 | <0.017% | 25.11 |
| niri / QML | 70.95 / 56.69 | 0.017% | 282.04 |

One final clean 60-second sample per row, after stopping a stale early QML
screenshot process. The prior three-pair PSS values (~58.6 MiB QML) are superseded;
shared pages/other allocation state affected them. No SwapPss. QML/niri used one
CPU tick, all other rows zero. Startup is process launch → first frame-callback
log observation on both, not physical presentation; cache state is not controlled.
Matched full-width 32px strips, scale 1; Hypr 1920×1080@60, niri 1278×692@60.
Hypr used a desktop-nested window; headless labwc gave no active Hypr output.
Hypr screenshots timed out for both clients; bounded timeout added, numbers retained.
Its nested backend also logged an xdg configure warning on preview shutdown.
Both Bead instances exit 0/surfaces=0. No daily shell/config changes. Tests: fmt,
clippy -D warnings, 2 geometry tests, actual lifecycle and measurements, cargo-deny;
Original Feature 1 CI passed; later identity repair preserved identical source trees.
strict license policy also correctly rejects GPL shell when deliberately selected.
408 new Rust lines plus small fixtures/runner updates; ~90 minutes including setup
and measurements. Installed cargo-deny 0.20.2 locally; no system packages added
(sudo unavailable; existing Rust/labwc reused). Reload/pacing/fonts are N/A here.
Verdict: surfaces on track; nested-Hypr testbed concern, no full-shell savings claim.
Next: Feature 2 drawing
and real 2880×1800 / scale 1.5 / 120 Hz text/emoji/shadow animation stress gate.


## Feature 2 — fractional drawing and damage (2026-10-13)

Bead draws rounded/bordered shapes, cached shadows, text/emoji, PNG/SVG and theme icons.
Orient colors are read from its existing JSON; no engine code was copied.
Fractional-scale + viewporter, direct SHM rendering and three reusable buffers work.
Damage improves CPU usage, but CPU drawing fails the real 120 Hz pacing target.

| Client / workload | PSS / USS MiB | CPU % | Draw p50 / p99 ms | Callback p50 / p99 ms | Startup ms |
| --- | ---: | ---: | ---: | ---: | ---: |
| Bead damage / animated | 42.49 / 22.30 | 51.30 | 3.82 / 23.63 | 8.68 / 25.51 | 400.11 |
| Bead full / animated | 42.44 / 22.24 | 59.18 | 4.94 / 24.61 | 8.93 / 29.05 | 404.08 |
| QML / animated | 105.60 / 87.35 | 5.75 | GPU; not compared | 8.33 / 9.22 | 143.25 |
| Bead / nested idle | 14.92 / 12.95 | <0.017 | N/A | N/A | 211.85 |
| QML / nested idle | 106.01 / 85.56 | <0.017 | N/A | N/A | 179.84 |

One 60s untraced CPU/memory run per animated row; draw statistics exclude first 2s.
Matched traced 10s runs supply callback intervals/startup (launch → callback log,
not photons). Client CPU is % of one core, excluding compositor/GPU. No SwapPss.
Measurements used the same light Orient fixture; default preview now matches active dark.
Live stress: 1100×590 logical panel; full 2880×1800 buffers, scale 1.5,120 Hz.
Bead presentation feedback p99 was 33.33 ms ON/OFF; budget 8.33 ms. Memory before/after
QML was121.79→105.60 MiB; trace run 126.36 MiB. PSS excludes some GPU allocations.
Idle: private labwc→niri, 1278×692/scale 1/60Hz, Noto fallback rather than unavailable
IBM Plex Sans. Separate scope from live stress. Both idle rows recorded zero ticks.
Live white overlays were stopped at owner request; no further live spawning.
Pixel equality tests cover aged buffers, RGBA/ARGB, text/images/shadows and clipping.
Nested niri RGBA/Hypr ARGB scale 1.5 static proofs passed. Protocol traces show only
three buffer allocations per stress run; zero full-frame intermediate copies.
fmt/clippy/tests/cargo-deny pass locally; CI status in STATUS. No system installs.
MPL option-ext is confined to GPL shell; [ttf-parser maintenance notice](https://rustsec.org/advisories/RUSTSEC-2026-0192.html)
exception is explicit in cargo-deny. Installed fonts only; SVG text/embedded rasters
not covered. Hot reload N/A. Resources/interface are a CPU proof, not shell parity.
~40 minutes build/measure/checks; 1155 net Rust lines plus 208 fixture/tool lines.
[Preview](examples/feature2.png) is a private nested screenshot, not the live desktop.
Verdict: **concern — CPU renderer misses120 Hz**. Propose the same panel on GPU before
Feature 3; STOP for owner feedback. Raw evidence retained locally under .research/feature2.
