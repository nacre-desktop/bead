# 0245 — Shell interaction CPU

Status: diagnosis complete; documentation only. The approximately 1%-of-one-core goal remains unmet.

## Method

Labels are `host/renderer/run`; H1 is the AMD integrated-GPU test host.
H1 uses a private headless parent compositor and nested stock compositor,
1920×1080, scale 1, 60 Hz, private HOME/XDG and activation-free D-Bus,
identical fonts, palette, static wallpaper, and synthetic service fixtures.
The copied private-session resource benchmark harness provides interactions,
process-tree CPU/PSS/context sampling, memory guards, cooldown, and cleanup.
Original harness files are untouched; raw evidence stays untracked.
Each arm has 60 seconds without the shell, 60 seconds initial idle, six
10-second interaction cycles, and 30 seconds rest, then UI evidence.
Each cycle opens/closes launcher and dashboard with 0.8-second open waits,
switches workspaces 2→1→3→1 with acknowledgements and 0.3-second waits,
then sends four notifications 0.2 seconds apart. Close/open transitions overlap.
These short arms do not remeasure the long-duration steady-idle advantage.

H1 host revision `74baf744d` uses framework `5c0e3136`; scratch `profiling`
inherits release, retains debug symbols, and sets `-C force-frame-pointers=yes`.
H1 captures use a private 4-GiB/no-swap scope, exclusive lock chunks below
590 seconds, and three cooldown readings 10 seconds apart within 2°C of baseline.
CPU is percent of one core for the shell process tree; host services are separate.
PPT includes CPU and integrated GPU. PPT, GPU busy and VRAM deltas are whole-host
interaction means minus each arm's no-shell baseline, not per-process readings.
Detailed diagnostics use a separate binary with logging-only frame identity,
geometry, timestamp and repair-area additions; these runs do not supply power results.

The host's `--renderer cpu|femtovg-gl` and `--panel-renderer` select
`bead_wayland::Options.renderer` for bar and panels; notifications remain CPU.
`BEAD_LAYER_REVEAL=0` holds animation policy constant. Setting it to `1` additionally
requires the reveal protocol and contour data; that compositor offload is unmeasured.

## Reproduction and renderer comparison

| Label | Shell CPU | PPT Δ W | GPU busy Δ pp | VRAM Δ MiB |
|---|---:|---:|---:|---:|
| H1/auto/base1 | 9.26% | — | — | — |
| H1/auto/base2 | 9.26% | — | — | — |
| H1/CPU/cpuA | 9.18% | +0.197 | +2.91 | +18.91 |
| H1/CPU/cpuB | 9.26% | +0.202 | +4.25 | +18.94 |
| H1/GL/glA | 9.16% | +0.247 | +5.95 | +64.43 |
| H1/GL/glB | 9.16% | +0.221 | +1.98 | +65.08 |

H1/auto/{base1,base2} proves SHM for bar, launcher and dashboard and satisfies the
agreed factor-of-two reference reproduction gate. Revision and settling windows differ
from the reference; this experiment does not isolate their effect.
H1/CPU/{cpuA,cpuB} versus H1/GL/{glA,glB}: median CPU is 9.22% versus 9.16%,
median PPT delta +0.199 W versus +0.234 W, and GL adds 45.8 MiB VRAM.
GL alone does not approach the goal; the small power difference is uncertain with
these samples and GPU busy varies widely. GL buffer transport is verified for all
core surfaces with no CPU fallback. Earlier H1/auto/{cpu1,gl1,cpu2} are excluded:
the first tmux launcher dropped renderer environment variables; all actually used CPU.

## Per-step accounting

All rows: H1/CPU/diag-cpu, summed over the six cycles. Close frames are assigned by
that surface's latest command, including overlap with the next panel's opening.
Pixels are millions: semantic damage / actual SHM repair. Full means whole-buffer
repair; the other frames use partial repair. Thread CPU excludes scene-worker work.

| Step / surface | Frames | Full / partial | Pixels M | Frame CPU sum ms | Frame wall median ms |
|---|---:|---:|---:|---:|---:|
| Launcher open | 121 | 6 / 115 | 62.70 / 63.22 | 88.63 | 0.507 |
| Launcher close | 24 | 12 / 12 | 12.90 / 12.90 | 33.03 | 1.171 |
| Dashboard open | 153 | 6 / 147 | 19.72 / 23.54 | 116.14 | 0.401 |
| Dashboard close | 57 | 5 / 52 | 31.56 / 32.66 | 29.10 | 0.432 |
| Workspace / bar | 180 | 0 / 180 | 0.224 / 0.305 | 34.66 | 0.189 |
| Notification arrival / bar | 34 | 0 / 34 | 0.056 / 0.083 | 2.66 | 0.060 |
| Notification lifecycle / overlay | 136 | 54 / 82 | 10.25 / 10.25 | unavailable | 0.208 |
| Settling / bar | 8 | 7 / 1 | 0.672 / 0.672 | 7.32 | 0.968 |

H1/CPU/diag-cpu core-frame wall splits, milliseconds summed: decoration 10.55,
pose 17.12, prepare 39.89, damage 14.19, repair/fallback 7.42, paint 208.72,
commit 15.75. These are render-thread stages, not end-to-end command latency.
Overlay render wall totals 51.75 ms; its stage/thread-CPU split is unavailable.
Overlay repairs are replayed from protocol buffer identities and semantic damage
using the pinned buffer-history/adoption algorithm; outside-repair copies total
0.761 M pixels. The CPU core paths log repairs directly. GL draws whole targets
in its early-return path, so CPU `UI_FRAME` stage counters do not cover GL frames.

## CPU profile and ranked causes

H1/CPU/perf-cpu: frame-pointer `cpu-clock:u`, 499 Hz, all process-tree threads,
user-only callchains, exact interaction window; 1,246 samples, none lost.
Process accounting is 2.68 seconds user plus 2.94 seconds kernel: 52.31% kernel.
Kernel profiling is forbidden by host policy; that share is not assigned to a function.
The table scales user samples by the measured 47.69% user share. Exclusive groups
partition sampled user CPU by nearest relevant caller; inclusive columns overlap.

| Group | Exclusive % shell CPU | Inclusive % shell CPU |
|---|---:|---:|
| Luau evaluation | 13.09 | 14.24 |
| Recovery persistence | 6.35 | 6.35 |
| Layout / UI construction | 5.01 | 18.49 |
| CPU rasterisation | 4.90 | 6.12 |
| Scene build / preparation | 1.99 | 18.68 |
| Wayland / event loop | 1.95 | 6.16 |
| Image / text | 1.53 | 1.99 |
| Damage computation | 0.08 | 0.23 |
| SHM copy; animation ticking | no exclusive samples | 0; 0.08 |
| Other / unresolved user; kernel | 12.78; 52.31 | — |

H1/CPU/perf-cpu top self hotspots (% user CPU): unresolved libc address 11.08,
Luau execute 5.30, free 2.17, write 1.77, malloc 1.52; blend-over is 2.33% in
perf's non-inline self report. Top useful inclusive stacks: generation materialize
39.33%, UI session update 39.00%, frontend build 29.13%, recovery save 13.32%.
Idle wakeups are not separately attributed by this interaction-only user profile.

Ranked cost centres overlap: recovery/write-heavy workers dominate; Luau/layout/scene
construction consumes 20.09% exclusive shell CPU; raster consumes 4.90%, mostly partial.
H1/CPU/task-cpu uses the primary binary plus an external 100-ms per-thread accounting
sampler: 18 threads exceeding 20,000 writes account for 59.36% of shell CPU,
including 45.63% in kernel time, and 99.80% of 1,791,931 write calls.
They account for 86.49% of kernel CPU; their work also includes UI construction.
Snapshots miss exited-thread tails, so these are observed lower bounds, not syscall
stack attribution. Recovery's 6.35% user share remains the directly sampled function cost.
H1/CPU/diag-cpu records 18 first-generation recovery saves in the interaction window,
1,813,962 write calls and 6,318,513 bytes, about 3.48 bytes/call including logs.
Fresh panel/overlay sessions repeat first-presentation saves; the fingerprint guard
only deduplicates inside one session. `serde_json::to_writer(File)` writes byte-vector
JSON without buffering. This explains a renderer-independent syscall workload,
but does not prove all kernel CPU belongs to recovery. GL's negligible CPU gain
is consistent with the relatively small measured raster share.

## Phase 2 plan

Savings below use H1/CPU/{perf-cpu,task-cpu}, not measured fixes;
percent means share of current shell CPU, and candidates overlap.

| Priority / candidate | Estimated saving | Power / memory trade-off | Risk |
|---|---|---|---|
| First: buffer recovery writes; preserve atomic rename and sync | Targets 45.63% observed kernel CPU in write-heavy threads; saved fraction unknown | Likely less CPU/PPT; small bounded writer buffer | Low implementation risk; preserve flush/error/durability semantics |
| Reuse unchanged recovery bundles and warm UI sessions/static layers across close | Up to 26.44% proven user share, plus unassigned kernel; depends on reuse coverage | Less rebuild/PPT; measure retained UI/assets memory | Stale state, resource invalidation, recovery files, lifecycle |
| Cache static UI evaluation/layout independently of animation pose | Up to 20.09% ceiling; target repeated materialization | Less CPU/PPT; retained scene/layout memory | Invalidation and dynamic bindings |
| Damage-only panel raster / avoid full repaints; cache static raster layers | Below 4.90% raster ceiling; existing partial repairs limit gain | Less CPU; cached pixels cost memory, copying costs bandwidth | Reveal contours, buffer history, stale/blank frames |
| Fewer animation frames | Below raster+animation work ceiling; no measured target | Less CPU/GPU/PPT, little memory change | High smoothness risk; preserve duration/easing |
| GL for animated surfaces / compositor reveal | GL alone measured 0.06 core-pp median gain; reveal saving unmeasured | Observed GL +45.8 MiB VRAM and +0.035 W median PPT | Driver/contour fidelity, first-frame and reversal correctness |

Run the smallest buffered-writer experiment first, then session/static-work reuse;
reprofile kernel/syscall costs before promising the target. Alternate controls,
measure CPU **and** PPT, verify smooth transitions, and retain the steady-idle lead.
