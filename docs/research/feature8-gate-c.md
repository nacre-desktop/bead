# Feature 7–8 / Gate C — 2026-10-15

Feature 7 is implemented; Feature 8 is at Gate C. **Recommendation: adjust**
for nested niri pacing and remaining hardware coverage. The new memory limit
passes. Stop here; no optimization pass or Feature 9 has been started.

## Scope and validation

Private versioned owner-only socket/client; 120 audited methods across 43 targets,
30 CLI-source locations and 12 shortcut declarations have typed ingress/selector
fixtures. Existing Python helpers remain external, using bounded argv, streams,
deadlines and matching reply IDs. File snapshots/watch/atomic replacement retain
last-good validated data. Native bar/state and sound routes work; future features
return unavailable. This is a compatibility catalog, not 120 implemented features.

The real panel controls the current default output: mute and volume click/drag/
arrow keys (5 percentage-point steps). No microphone or output-device chooser
is claimed. Open through the mapped `nacre popout/controls sound` or
`drawers toggle sound` routes, using the explicit private socket. Escape, close
button, outside click, focused activation and rapid reversal are tested. Small
durable open/page state survives reload, worker recreation and SIGKILL relaunch.

Additional Gate C requirements are done: facade-only framework imports plus a
non-Nacre example; Rust-generated Luau SDK and analyzer CI; native core and ext
clipboard with independent UTF-8/plain-MIME peers; screen-free `--check`;
numeric-only `--report` (30.001 seconds, mode 0600, includes closed-view history).

Local: 70 tests, fmt/clippy and CPU-only check pass. CI also links the CPU-only
headless binary, checks all features, runs the analyzer, service lifecycle and
license graphs. Source [fdc99cd](https://github.com/nacre-desktop/bead/commit/fdc99cd45f8d34b355df4dc24af93afcf8b46fb8)
passed [CI 38719326494](https://github.com/nacre-desktop/bead/actions/runs/38719326494).
Parallel and one serial 50 ms Luau deadline failures under host CPU/I/O pressure
are retained; the isolated repeat and CI passed without increasing that limit.

## Memory and idle CPU

MiB = 1024² bytes. PSS is proportional resident process memory and includes all client threads.
SwapPss is proportional swapped memory; DRM is deduplicated per
client. CPU is one-core percentage over 60 seconds; one scheduler tick is
0.0167 percentage points here. PSS+DRM is the requested metric, not exclusive
VRAM: system-memory DRM can overlap resident accounting. SwapPss is shown
separately because host reclaim affected the Hyprland run.

| State | Hypr PSS+DRM | Hypr SwapPss | niri PSS+DRM | niri SwapPss |
|---|---:|---:|---:|---:|
|Bar only|21.35|0.00|22.64|0.00|
|Warm panel, after 60 s|22.00|7.30|32.75|0.00|
|Closed, after 60 s|11.13|4.79|26.13|0.00|
|After output/rapid/keyboard lifecycle|16.05|2.67|24.56|0.00|

Every ten-cycle immediate closed-idle sample, the 60-second closed sample,
and the final lifecycle sample satisfy **bar-only + 5 MiB**, with the original
cutoff unchanged. Niri’s largest immediate delta is +4.01 MiB; closed 60 s is
+3.50 MiB and final lifecycle is +1.92 MiB. Hypr remains below its initial
bar baseline even when proportional swap is added back.

A finished bar can draw from retained rasters without font/glyph scratch. Closing
the view drops its VM, worker, rasters, SHM pools and renderer, and puts the bar
baker into idle mode. After any later bake, one 250 ms quiescence timeout frees
unused scratch, then blocks again. No polling loop, allocator replacement or
unsafe driver-page eviction. This also explains why closed memory can be lower
than the initial bar warmup. A trim test proves visible pixels survive and changed
assets can still rebuild.

| 60 s CPU | Hypr | niri |
|---|---:|---:|
|Bar only|0.0167%|0.0167%|
|Panel open|0.0167%|0.0333%|
|Panel closed|0.0167%|0.0000%|

No persistent application helper children. Script/raster workers are threads
already included above. The private PipeWire fixture is separate infrastructure:
2.14/2.15 MiB PSS (Hypr/niri), excluded from client totals; the compositor and
input/clipboard proof peers are excluded too. Native CPU scenes have no app DRM
descriptors. Final FDs/threads return to 41/10 on Hypr and 42/10 on niri.

## Warm comparison and cost

The matched output-controls QML subset uses unchanged audited Nacre widgets,
a 400×286 body / 464×350 outer surface, one workspace and no focused title,
with static service shims. It omits microphone/device lists, uses text buttons
where the native kit uses icons, and is not a pixel-identical full shell. The
native side uses real private PipeWire. QML’s transport-shim cost underestimates
a full service-driven shell. The presentation probe is included but has no idle
reporting timer.

| Warm bar + output panel | PSS+DRM | SwapPss |
|---|---:|---:|
|Native Hypr|22.00|7.30|
|Native niri|32.75|0.00|
|QML Hypr|163.09|0.00|
|QML niri|159.07|0.00|

QML idle CPU was 0 ticks in each 60-second sample. Do not treat those finite
samples as an invariance proof. Native warmed resident+swap is 29.31/32.75 MiB.

Feature 6’s ~104.90/105.53 MiB was a larger native CPU-bar + GL stress panel,
1100×590 content / 1164×654 outer geometry. Accepted 6b’s same stress scene
rose to ~143–153 MiB: CPU-first raster/allocator residency coexisted with warm
GL allocations. The new actual 400×286 panel is smaller and stays CPU, so it
does not show that increase. These are different workloads; no full-shell or
like-for-like 70% optimization claim is made. Explicit GL for the small panel
was also tested: after display/context termination it still kept ~20 MiB above
bar-only (Mesa/LLVM mappings plus anonymous/heap pages), so it fails this gate.

## Presentation and responsiveness

Same private containment, 2880×1800 at scale 1.5; Hyprland virtual 120 Hz,
nested niri 60 Hz. CLOCK_MONOTONIC compositor feedback is used on both sides,
not first callback or QML component completion, and not physical scanout.
Native opening intervals use a declared 220 ms window around the 200 ms motion;
closing intervals use close-to-release phases. QML has 20 tagged 200 ms slide/
fade phases. Long idle gaps are not compared as animation frame periods.

| Presentation interval p50 / p99 ms | Native open | Native close | QML open | QML close |
|---|---:|---:|---:|---:|
|Hypr|8.33 / 8.56|8.34 / 8.74|8.36 / 86.11|8.37 / 24.74|
|niri|16.23 / 41.34|16.21 / 47.27|16.15 / 43.70|16.13 / 32.99|

Native longest opening/closing intervals: Hypr 9.75/16.33 ms; niri 45.95/
50.83 ms. Native draw p50/p99: Hypr 2.42/6.37 ms (max 7.91), niri 2.22/
7.25 ms (max 7.27). Hypr mostly follows virtual 120 Hz; one closing miss
remains. Niri’s tails exceed its frame budget despite shorter CPU draw work;
queue/callback/presentation effects remain. QML also has tails, especially
Hypr remapping; retain them rather than substituting a favorable median.

Native command-start → ready UI presentation p50/p95: Hypr 48.63/54.39 ms,
niri 74.88/87.65 ms (ten fresh worker lifetimes). This includes CLI/transport
and preparation. Matched cold process startup / QML reload is N/A here: the
reference starts bar+panel together and retains QML objects, so its component
completion would be a different boundary. Warm QML animation is not a native
cold-worker startup comparison.

Final five native reload file→presentation samples (50 ms debounce included):

- Hypr: 66.97, 59.54, 56.48, 63.35, 64.49 ms.
- niri: 60.77, 60.35, 58.11, 57.74, 56.22 ms.

Invalid edits keep the live UI, dependency recovery retains page state, closed
reopen retains page, and SIGKILL relaunch restores open/page on both. No-change
accepted generations still request real presentation feedback. All ten reopens
also passed a visible-track pixel check (minimum 3476 bright pixels), not merely
log markers or hit testing. A blank-body experiment and +5.13/+5.03 MiB memory
failures are retained; they are not final passing visual evidence.

## Gate limits and reproducibility

Recommendation is **adjust**, not acceptance of uninterrupted pacing. Keep 6b
step 0a nested niri pacing and strict idle-invariance items open. Keep the
29.61→32.66% typing CPU regression on the later optimization-pass list. Real
niri output disappearance is unverified: nested output-off left the global
present. Hypr real private output move/removal passes. No physical/live test,
installation, IME/Orca/NVIDIA acceptance, notification takeover or Feature 9.
The catalog is partial operational parity; for example windowTitle state is a
focused-title projection with retained=false, not full per-output retention.

Evidence (ignored local `.research/feature8/`): `idle-final-{hypr,niri}`,
`qml-final-{hypr,niri}`, `reload-final-{hypr,niri}`, `report-final-hypr`,
`scope-trim-hypr`, and the retained failed/diagnostic runs. Reproduce through
`spikes/measure.py --bar-ui` with `BEAD_HARDENING=panel`,
`panel-reference`, `panel-reload`, or `diagnostics`; native final uses
`BEAD_PANEL_IDLE_SECONDS=60`, `BEAD_PANEL_RAPID=1`, and niri
`BEAD_BAR_MATRIX=1`. Use the explicit audited Nacre archive and compiled
`spikes/build-qml-probe.sh` for the reference; see scripts for exact variables.
All builds use the shared target, nice -n 10 and Cargo -j2. No local builds
ran during final timed samples.

Versions: Rust 1.99.0, niri 26.04 (8ed0da4), Hyprland 0.56.2 (efb5099),
Quickshell 0.3.2, Qt 6.12.0, Luau analyzer/runtime 0.740, Pillow 12.3.0.
Reference Nacre revision: 68df8dd3b25cbf202d3859d0178a50cceffbea0d.
Release client SHA256:
`3e41cf397156a68aab513e8273612645c88ad349830069aa8c1904d7e06e50db`.

From accepted 6b to this source: 3148 Rust lines added /257 removed (includes
facade import changes), 1942 generated lines (catalog1862 + SDK80), and
537/3 proof-script additions/removals. This task continued on 2026-10-15;
process idle intervals are measured 60 s, not a one-hour endurance result.
The Feature 7 source checkpoint was 06:36 UTC; final source was 10:18 UTC
(about 3 h 42 m between source checkpoints, including rebuilds/failed probes).

## Owner decision and supervised physical check — 2026-10-15

Gate C **accepted** under the owner's adjusted ≤16.7 ms opening/closing p99
criterion. One explicitly confirmed live niri run: 20 IPC-driven cold cycles,
2880×1800, scale 1.5, 120 Hz; CLOCK_MONOTONIC presentation feedback,
refresh 8.333313 ms. Opening p50/p95/p99 8.333/8.336/16.667 ms (466 periods),
closing 8.333/8.336/8.337 ms (470). Longest opening/closing 58.330/8.338 ms.
Nested niri's 41.34/47.27 ms p99 is not reproduced on physical scanout;
no universal uninterrupted-120-Hz claim. Cold ready p50/p95 45.25/64.64 ms.

Passive mode now covers dynamic-panel focus and dismissal input regions.
Private D-Bus/XDG/PipeWire, no real input injection, capture, control changes
or installation. Exact owned client/PipeWire PIDs exited; supervisor PID file
removed. Private activation's failed GVfs FUSE mount did not prevent all
presentation/state checks; bus-disconnect message followed normal cleanup.
PSS+DRM bar/closed 23.03/25.14 MiB, no swap. Short 2 s CPU samples ~5%
include live-backend updates; do not compare them with 60 s nested idle CPU.
The single physical authorization is consumed. Optimization is now authorized;
Feature 9 remains outside scope. Raw evidence: .research/optimization/physical-niri.
