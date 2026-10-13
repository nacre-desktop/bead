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


## Retained renderer comparison — before Feature 3 (2026-10-13)

All three paths now animate one cached panel picture, never its content each frame.
Content-sized surfaces save memory; startup profiling fixed the large shadow bake.
Femtovg/wgpu is the recommended next renderer; tiny-skia remains the no-GPU fallback.
Virtual callback tails still fail a strict 120 Hz gate, so this is an adjust verdict.

| Renderer/surface/run | CPU % | Draw/submit p50/p99 ms | Callback p50/p99 ms | PSS/USS MiB | DRM total MiB | Startup ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| cpu/full/first | 38.78 | 5.105/21.413 | 8.459/36.120 | 31.41/21.32 | no app GPU fd | 178.88 |
| cpu/full/later | 54.05 | 4.693/15.430 | 8.419/17.017 | 31.27/21.27 | no app GPU fd | 110.24 |
| cpu/content/first | 47.45 | 4.544/16.545 | 8.533/19.648 | 21.56/18.06 | no app GPU fd | 156.29 |
| cpu/content/later | 53.75 | 4.769/13.955 | 8.471/15.807 | 21.23/17.86 | no app GPU fd | 83.86 |
| femtovg/full/first | 2.95 | 0.328/0.766 | 9.420/119.575 | 45.38/44.15 | 162.40 | 281.08 |
| femtovg/full/later | 1.92 | 0.313/2.546 | 9.980/184.044 | 38.81/37.52 | 162.40 | 110.22 |
| femtovg/content/first | 4.62 | 0.185/0.591 | 8.345/11.524 | 45.73/44.20 | 121.05 | 223.29 |
| femtovg/content/later | 5.42 | 0.272/0.592 | 8.344/10.756 | 36.00/32.11 | 121.05 | 109.99 |
| vello/full/first | 4.77 | 0.896/1.275 | 16.317/108.184 | 151.64/148.03 | 429.92 | 839.94 |
| vello/full/later | 1.37 | 1.030/2.568 | 67.240/408.570 | 24.37/20.77 | 429.92 | 433.07 |
| vello/content/first | 6.82 | 0.780/1.354 | 9.326/74.304 | 155.09/151.23 | 388.57 | 1207.59 |
| vello/content/later | 1.88 | 0.942/1.868 | 24.502/676.600 | 45.88/42.07 | 388.57 | 619.55 |
| qml/full/first | 39.88 | N/A (trace control) | N/A | 84.44/71.81 | 220.21 | 330.14 |
| qml/full/later | 28.25 | N/A (trace control) | N/A | 83.68/70.11 | 220.21 | 183.31 |
| qml/content/first | 34.17 | N/A (trace control) | N/A | 92.51/76.15 | 106.04 | 330.85 |
| qml/content/later | 22.28 | N/A (trace control) | N/A | 82.62/71.78 | 106.07 | 223.74 |

Private labwc→Hypr virtual2880×1800/1.5/120Hz; refresh feedback0, not physical scanout.
Each row60s after5s warmup, common CPU-baked picture; GPU times are CPU submission,
not GPU execution. Full buffers2880×1800; content1746×978. First private caches fresh,
later reused; OS caches uncontrolled. Other busy QML/GPU tests ran; no processes killed.
Qt traces affect CPU (not ranked); startup first populated callback. DRM system0
totals deduplicated, not VRAM; fonts/emoji retained in picture;
PSS overlaps/misses allocations. No SwapPss.
Old shadow342ms→12ms; old font-scan786ms outlier/repeats13ms; select5 files/14 faces
lazily in GPL shell. Initial default-AA Vello728ms/223PSS/389DRM, peak293MiB retained.
First-pass Vello alpha wrong; corrected transfer pixel-matches all3,4 rows rerun.
Exact retained partial/full pixels and≤2/255 shadow; no-GPU fallback and8 checks pass.
No system installs; ~4h elapsed; 1122 net Rust lines;
raw evidence .research/retained-comparison. [Private preview](examples/retained-renderers.png).
Verdict: **adjust — choose femtovg + small retained surfaces; keep CPU fallback**.
No physical120Hz pass claimed; Vello's first/large-surface costs reject it here. STOP.
