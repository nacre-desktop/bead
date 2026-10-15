# Optimization pass — complete

Gate C is accepted. The optimization pass is complete; stop before Feature 9.
No installation, release, reference-repository changes or further live test.

## Physical Gate C decision

The owner confirmed the one supervised live niri run immediately before it.
Twenty cold open/close cycles at 2880×1800, scale 1.5, 120 Hz passed the
adjusted ≤16.7 ms p99 gate. Opening p50/p95/p99: 8.333/8.336/16.667 ms
(466 periods); closing: 8.333/8.336/8.337 ms (470). Longest: 58.330/8.338 ms.
CLOCK_MONOTONIC feedback reported an 8.333313 ms refresh period. Cold ready
p50/p95 was 45.25/64.64 ms. This does not claim uninterrupted 120 Hz.

Private state/bus/PipeWire, passive overlay and IPC only; no input injection,
capture, real audio/config changes or installation. Owned processes exited,
PID file removed. Authorization consumed. Optimized source was tested nested.

## Latency and preparation

Matched nested geometry: 2880×1800/1.5. Hyprland uses virtual 120 Hz; niri's
nested Winit path runs at 60 Hz. Twenty cycles per case, no concurrent Bead
builds/aging. Ready means command-start → presentation of the first validated
UI generation. It includes CLI/IPC and is not merely a frame callback.

| Ready p50 / p95, ms | Hyprland | niri |
|---|---:|---:|
| Fresh cold baseline |70.27 /106.84|98.55 /102.93|
| Selected source, cold |65.02 /105.93|106.93 /111.24|
| Selected source, 150 ms hover lead |15.67 /17.18|13.20 /17.89|

Audio-icon entry starts the existing script/bake worker with closed state,
the hovered output and its scale. It creates no panel/dismissal surface.
Opening uses the preparation; unused work expires after 2 s. Remaining in
the icon does not renew the deadline. Worker, VM and rasters are dropped;
no persistent warm cache. Unused-hover 60 s memory deltas were −0.75/−0.92 MiB.
The journal stayed closed. Every measured reopen has visible-slider checks.

The first final proof incorrectly required a commit after open, timing a later
visibility update (32.67/74.34 ms p50). Preparation intentionally commits before
open. The corrected proof requires a new presentation after the command with
an exact earlier generation/update, companion and source_error=false match.
Wrong updates, future commits and source errors fail. Both corrected 20-cycle
runs passed; explicit command/presentation timestamps are saved. Earlier data
remain under quiet-hover-*; corrected timing is quiet-first-present-hover-*.

| Opening /closing presentation p99, ms | Hyprland | niri |
|---|---:|---:|
| Fresh cold baseline |21.55 /9.17|33.85 /33.68|
| Selected prepared panel |9.04 /8.81|34.44 /33.95|
| Prepared longest interval |17.01 /9.08|37.81 /34.04|

Nested niri median cadence varied between ~16.6 and ~33.3 ms across identical
prepared binaries; its p99 stayed ~34 ms. This remains phase-sensitive nested
pacing evidence, not optimized physical scanout validation. Keep 6b step 0a's
nested niri/shared-GL pacing and strict idle-invariance items open.

## Memory and idle acceptance

All numbers below include PSS+DRM+SwapPss. Selected-source samples had no swap.
Active warm is the median after opening, before long-idle reclamation. The
400×286 content/464×350 outer CPU panel is the same scene across these runs.

| Selected source, MiB | Hyprland | niri |
|---|---:|---:|
| Bar, 60 s idle |20.13|21.48|
| Active bar+panel, median |26.67|29.03|
| Panel open, 60 s idle |26.18|28.69|
| Closed, 60 s idle |21.32 (+1.18)|22.05 (+0.57)|
| Post-lifecycle closed |22.20 (+2.07)|22.25 (+0.77)|

All 20 individual release cycles, closed 60 s and final lifecycle checks pass
within the matched bar+5 MiB, including swap. Reopens, controls, native clipboard,
all dismissals and rapid latest-intent visibility pass. Hypr real private output
move/removal passes; nested niri does not prove real output disappearance.

Fresh baseline active warm medians were 32.73/30.89 MiB. Feature 6's ~105 MiB
used a larger GL stress panel (1100×590 content), not this small CPU scene.
Accepted 6b's 143–153 MiB stress increase came from CPU-first rasters/allocator
and warm GL coexisting. Do not claim a like-for-like ~75% renderer saving.
The earlier small-GL experiment retained ~20 MiB above the bar after EGL shutdown
and trimming, including Mesa/LLVM mappings. Policy: CPU for small panels; GL
only for large animated surfaces. Do not discard driver-owned mappings unsafely.

60 s idle CPU, one-core percent (bar/open/closed): Hypr 0.017/0.050/0.017;
niri 0/0.050/0.033. Tick resolution is 0.017 percentage points per minute.
These samples do not establish strict idle invariance. File-page reclamation
and SwapPss explain why some long-idle baselines are below active warm memory.

## CPU and retained changes

Recorded physical field differences showed 599 unfocused title changes driving
600 window-list changes/603 bar updates without a focused-title change. The bar
did not read that model. An A/B/B/A replay with the real bar+panel delivered
200 backend updates in each 20 s trial. Host models now stay current without
view preparation when no binding reads them. Used/computed dependencies still
stage atomically; rejected branches retain prior reads. JSON and cache overflow
use conservative staging. Future bindings/reloads see the latest model.

| CPU, one core, matched comparisons | Hyprland | niri |
|---|---:|---:|
| Background title churn, before→after |3.70→0.45%|3.43→0.25%|
| Final maximum-rate typing, before→after |59.85→59.54%|56.60→57.17%|

Background updates caused ~200→0 bar commits, except one legitimate clock.
Genuine focus changes committed and presented on both. Fixed separate probe
executable avoids client/probe executable-page sharing bias.

The final typing A/B/B/A does **not** demonstrate a meaningful improvement.
CPU ms/operation: 38.82→38.89 Hypr; 180.00→183.56 niri. Historical
29.61→32.66% came from different runs and remains an unresolved optimization
item. Early isolated bytecode trials helped Hypr, but no net final typing win
is claimed. Keep the final negative result visible.

Other retained changes: generation-owned markup/kit/SDK bytecode with fresh
sandbox evaluation (8 MiB bytecode budget; existing 50 ms/8 MiB Lua limits);
shared scoped watcher (one thread/three FDs fewer, ~2.4 MiB warm in its trial);
codegen-units=1 (1.6–1.9 MiB less mapped code, executable 25.98→20.02 MiB).
A stress-discovered closed-error overlay now clears stale layer ranges and
prepared frames while preserving last-good body/state. Regression failed
before the fix and passed afterward, with and without prepared frames.

## Rejected candidates and one-hour aging

| Candidate | Evidence / decision |
|---|---|
| Fewer whole-scene copies |No useful typing gain; reverted.|
| Two SHM buffers |Nested niri exhausted compositor-owned slots; retain three.|
| 512 KiB thread stacks |No consistent PSS saving; retain normal stack headroom.|
| 16 versus 64 MiB raster/GL-image cap |Active scene below budget; no useful gain.|
| Zero GL depth/stencil |Round slider became square; reject despite lower memory.|
| Zero depth, stencil retained |Driver selected the same 24/8/0 on both; no saving.|
| MSAA |Already zero samples.|
| Thin LTO |No mapped-memory saving in the actual scene.|
| Strip-all |Disk-only reduction, removes symbols; not selected.|
| panic=abort |Disables tested worker-panic→Fatal contract; retain unwind.|
| Image decode sizing |SVG already display-sized; no large JPEG/PNG hotspot here.|

Eight aging runs completed 3600.70–3606.47 s of actual open/release use,
370–372 cycles, plus 60 s endpoint idle. PSS+DRM+SwapPss start-bar/hour-warm/
closed MiB are below. Same older-baseline executable shared pages across eight
clients: these small concurrent PSS figures are not standalone bar measurements.

| Allocator | Hyprland | niri |
|---|---:|---:|
| System |7.89 /18.16 /11.72|8.45 /20.29 /11.99|
| Arena2 |7.77 /16.30 /9.96|8.32 /17.62 /7.21|
| mimalloc 3.5.3 |32.53 /51.06 /51.13|31.09 /52.66 /50.33|
| jemalloc |44.55 /25.67 /16.58|16.02 /21.65 /8.99|

Default client-only preloads retained the existing glibc trim lifecycle, not
allocator-specific purge adapters. Concurrent stress logged 14 deadline,
4 layer-range and 4 post-failed-load state error lines (duplicates possible).
These are memory-aging results, not functional/pacing passes. Initial soaks
that ended early remain separate. No budget raised or failure hidden.

Arena2 did not help the **selected combination** in its separate interaction
trial: closed Hypr 19.73→20.95 MiB, niri 20.27→21.17. Keep the system allocator
without a dependency, environment override or mallopt policy. Patches and failed
experiments remain in ignored evidence; rejected product hooks were removed.

## Validation and reproducibility

75 workspace tests, fmt/clippy, default and linked CPU-only release builds pass.
Official Luau analyzer checks 18 real modules and the generated Rust SDK.
Final quiet proofs: 20 cold and 20 prepared cycles per compositor with pixel
checks; controls/native clipboard/dismissals; rapid intent and lifecycle;
five exact-update reloads plus invalid/dependency recovery and SIGKILL restore.
Reload file→present p50/p95: 65.30/71.64 ms Hypr, 62.64/71.70 niri, including
the unchanged 50 ms debounce. Numeric-only 0600 report ran 30.00174 s and
retained closed-view history. Executable source d72a55f passed CI 38733902941.

Evidence: ignored .research/optimization, especially quiet-final-summary.json,
aging-summary.json, quiet-* and quiet-first-present-hover-*. The default and
CPU-only variants used the existing shared Cargo target, nice10/Cargo-j2.
The default executable was restored and matched its saved hash. No physical
optimized-run, IME/Orca/NVIDIA or real niri hotplug claim. Stop before Feature 9.
