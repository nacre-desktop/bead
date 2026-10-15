# F5 cache follow-up

Owner approved this follow-up before Features 9–10. Cache admission uses one
192 MiB process-wide ceiling, plus smaller owner limits. This is a ceiling on
retained cache payloads and estimated collection metadata, not process RSS/PSS,
allocator arenas, active store data, source bundles, SHM or driver memory.
`--report` includes `cache_bytes`: process limit/usage and each live cache owner.
Missing dependency-internal byte measurements are not represented as zero.

Shared raster charges follow their last retained reference, including prepared
frames and native GL upload references. Removing a lookup does not release its
charge prematurely. Exclusive caches evict least-recently-used entries before
admission; a single-generation value cache drops its old value. Optional
computed/markup/materialized/selector accelerations bypass storage when too big.
An active raster working set that cannot fit returns an error through the
host error path, rather than silently discarding a visible resource.

| Cache | Owner ceiling |
| --- | ---: |
| Retained CPU pixels, legacy CPU textures, per-scale pixels | 64 MiB each |
| CPU composite, native GL images/composite, opt-in wgpu images | 64 MiB each |
| Reachable Swash glyph images and usage metadata | 8 MiB |
| Retained nodes / prepared frame metadata | 8 MiB each |
| Compiled markup / markup values / computed values / materialized view | 8 MiB each |
| Font selector | 512 KiB and 8 selections |
| Per-scale lookup metadata | 64 KiB and 8 scales |

Byte figures include conservative payload/collection estimates, excluding
allocator headers. Shared pixels are charged once; some immutable view metadata
is deliberately overestimated across caches. The global ceiling applies across
owners, workers, scales and surfaces; independent 64 MiB limits cannot multiply
without bound. Cache hits update recency. No allocator or font engine rewrite.

## Dependency-owned storage

Swash exposes image and outline caches. Images are byte-accounted and evicted by
last glyph use; unused outline caching is cleared. Swash's scaler context and
cosmic-text's private font/match/shaping internals expose no complete byte API.
The preparation worker resets its CPU rasterizer on idle (and again after the
250 ms settling window), retaining only prepared visible pixels. A panel worker
and its frontend/value caches are dropped on close. The legacy whole-layer path
uses a temporary rasterizer. These internals have externally bounded lifetimes,
not a claimed measured internal byte cap. A direct CpuRenderer host must drop
or recreate it at its own idle/close boundary too.

Native GL/wgpu image requests count pixel bytes and metadata. Opaque driver
allocations, femtovg internals and Vello internals cannot be byte-capped through
those APIs; releasing the per-surface renderer on idle bounds their lifetime.
PSS+DRM measurements remain required, including Mesa's previously observed
roughly 20 MiB residue. Small panels and bars remain CPU rendered.

## Validation and measurements

The headless stress fixture churns real Luau/store values and legacy CPU text
rasters past the 64 MiB cache boundary, checking every live owner and the global
ceiling throughout. Admission tests cover recency and shared-reference lifetime.
Existing goldens, pose counters and service dependency boundaries remain active.

## Memory result: hold for the owner

The cache candidate is **not integrated**. The owner's brief requires stopping
if a gap fix regresses the accepted Gate C/optimization memory figures. First
open rose in the final matched pair, although later opens became smaller and
all close/lifecycle checks passed. Keep this checkpoint for review; do not
resume opaque-composition work or Features 9–10 until the owner decides.

Existing nested harness, real bar plus sound panel, CPU renderer, 2880×1800,
scale 1.5. Hyprland virtual120 Hz; niri nested60 Hz, not physical120. Three
open/close cycles, five-second idle windows. Baseline is the saved pre-cache
F5e release (8e11119; the OSD-only fix does not change this scene). Candidate
is this checkpoint. Each compositor stays alive across the pair: H baseline
then candidate, N candidate then baseline. Both have identical runtime options;
report capture is tested separately. Fresh private state/cache directories and
a neutral pointer prevent restored/hover-prepared panels contaminating bar-only
samples. A strict assertion checks closed/released state before that sample.

| PSS+DRM MiB | Hyprland before → after | niri before → after |
| --- | ---: | ---: |
| Bar only | 21.834 → 21.902 | 23.509 → 23.629 |
| First warm open, after ready+0.4s | 27.848 → 28.482 | 30.854 → 31.259 |
| Third warm open | 28.035 → 26.741 | 30.101 → 29.382 |
| Closed, five seconds idle | 22.602 → 22.986 | 23.077 → 23.761 |
| Final lifecycle delta from own bar | +1.643 → +1.521 | −0.432 → −0.082 |

Every close/release/lifecycle stays within its own bar+5 MiB; no DRM client fds
were observed. First-open delta is +0.634/+0.405 MiB; third-open delta is
−1.294/−0.719 MiB. Other tasks were active, so these are contended short memory
checks, not CPU, idle-invariance, QML parity or presentation acceptance. Earlier
independent runs and diagnostic pairs remain recorded, including incomplete
runs. Absolute historical PSS varies with shared file and buffer residency.

The five-second first-warm split also needs investigation: H anonymous PSS fell
0.671 MiB and shared SHM rose0.801 MiB; N anonymous PSS rose1.469 MiB, with the
same SHM figure. These are a later boundary than the ready+0.4s table row. This
is not evidence that the whole increase is only code pages or only a cache leak.

111 workspace tests, fmt, all-features clippy with warnings denied, CPU-only and
release builds, cargo-deny, service dependency boundary and 22-module Luau
analysis pass. Tests cover byte/LRU admission, cross-owner global admission,
actual legacy-texture churn past64 MiB, and a prepared frame retaining its charge
after the cache lookup is dropped. A separate native report proof checks each
owner's ceiling and that its sum equals the process total (188897 bytes in that
sample, ceiling201326592). Private dependency internals remain N/A as above.

The recovery found an empty measurement script after an interrupted write;
published contents plus the intended priority floor were restored and checked.
The harness now enforces absolute niceness≥10. It also avoids private GVFS/FUSE
activation, isolates persisted panel state per variant, and validates initial
closed state. The existing report30s default had overridden explicit --seconds;
that option now works and the unchanged default remains30s. No live service,
configuration, installation or release was touched. No task processes remain.

Owner decision: accept the first-open tradeoff, or require it removed before
integration. Recommended: adjust the first-open retention before resuming UI
features. Opaque CPU comparison, Feature9 matched QML, Feature10 real helper,
protocol adapter and GateD remain outstanding. Published Nacre5995d171 still has
no implemented v1 package/goldens to pin at this checkpoint.
