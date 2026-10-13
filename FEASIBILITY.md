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
GitHub CI 38391925119 passed for feature code 8a5069d.
strict license policy also correctly rejects GPL shell when deliberately selected.
408 new Rust lines plus small fixtures/runner updates; ~90 minutes including setup
and measurements. Installed cargo-deny 0.20.2 locally; no system packages added
(sudo unavailable; existing Rust/labwc reused). Reload/pacing/fonts are N/A here.
Verdict: surfaces on track; nested-Hypr testbed concern, no full-shell savings claim.
Next: Feature 2 drawing
and real 2880×1800 / scale 1.5 / 120 Hz text/emoji/shadow animation stress gate.
