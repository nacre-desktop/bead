# B0126 — instant slider drag

Status: shipped by default through `ef9d5c01`. The final integrated CPU build
passes the current QML-relative p95 and two-frame p99 gates at both 60/120 Hz,
plus five-pair relative memory, first-key and shared fill/handle checks. All five
steps passed full remote Arch CI and exact-commit GitHub CI. These are private
synthetic presentation-feedback results; physical scanout, Intel/laptop latency
and the absolute laptop memory cap remain unmeasured.

The preserved
[patch](evidence/0126/compositor-knob.rejected.patch) applies to `61e73507`.
It records the measured implementation; the product version omits diagnostic renderer pins.

The historical gates were pointer→present p95 ≤21 ms at 60 Hz and ≤12 ms at 120 Hz,
moving pixel error at 2000 physical px/s strictly below QML, first-key UI CPU
≤8 ms, and ≤1 MiB added unreclaimable memory. Unreclaimable means anonymous
+ SHM + DRM + SwapPss. File-backed PSS is informational. Five alternating
control/candidate pairs are required for memory acceptance.

## Latest continuation: QML-relative gates

The first masked drag frame originally took 12.095 ms. Its prepare stage was
11.681 ms wall / 11.630 ms CPU: one image layer cost 8.225 ms (7.732 ms image
resources), and 59 text layers cost 3.041 ms. Source maps the root image to the
generated joined-frame SVG rasterized by resvg; the asset path itself was not
logged. These nested spans overlap prepare and must not be added twice.
`6528a057` retains immutable prepared rasters through masking and preallocates
the two small child SHM slots. The integrated first masked frame is 0.562 ms,
including 0.0037 ms prepare and 0.452 ms composition. Pre-draw mask/allocation
spans are 0.127/0.052 ms. The parent retains its existing reusable three-buffer
pool and damage history; steady slider motion allocates no SHM buffers.
[Before/final wall and thread-CPU spans](evidence/0126/qml-parity-profiles.json).

`8486ac8f` adds a local fixed-label patch without Luau/layout. Its first build
regressed to 18.816/18.985 ms p95/p99 at 120 Hz and 42.383 ms p99 at 60 Hz.
`27580f9a` removes unchanged anchor/margin commits and flushes local values after
label-parent paints, preserving one outstanding frame callback. Its isolated
stage passed both gates (60 Hz p95/p99 26.260/26.626; 120 Hz 11.746/11.957 ms).
Label-associated damage in that trace was at most 8,960 pixels, 0.68% of the
panel; changed value-text baking was about 0.06 ms p95. The final integrated
source includes the intervening launcher/UI changes; its validation tree equals
`ef9d5c01` apart from the archive's `REVISION` record.

Three predeclared alternating QML/Bead pairs per rate retain all twelve gestures
in every arm, without concurrent screenshots. All exact joins are pooled using
nearest-rank quantiles. The output is queried at 2880×1800, scale 1.5, on private
direct labwc with synthetic feedback. This is not physical niri scanout.

| Rate | Bead joins; p50 / p95 / p99 ms | QML joins; p50 / p95 / p99 ms | Current gates |
| --- | ---: | ---: | --- |
| 60 Hz | 291; 18.267 / 26.271 / 26.517 | 326; 18.177 / 26.801 / 34.256 | p95 ≤ QML; p99 ≤33.333: pass |
| 120 Hz | 543; 10.126 / 11.764 / 11.875 | 576; 10.131 / 11.823 / 12.044 | p95 ≤ QML; p99 ≤16.667: pass |

| Rate | Input→dispatch p95 | Dispatch→commit p95 | Commit→feedback p95 |
| --- | ---: | ---: | ---: |
| 60 Hz | 0.043 ms | 10.594 ms | 15.998 ms |
| 120 Hz | 0.043 ms | 4.102 ms | 8.016 ms |

Dispatch→commit includes waiting for the one-frame callback credit. Stage
percentiles are not additive. CPU raster work is now confined to the slider and
changed label; the full frame image is reused.

The 120 Hz p95 margin is small (0.059 ms); individual-run spreads remain in
[all arm results](evidence/0126/qml-parity-latency.json) and
[all joined sample durations](evidence/0126/qml-parity-latency-samples.json).
Earlier concurrent-screenshot stress tails remain below; they are not silently
replaced or removed from those datasets. The historical 21 ms absolute 60 Hz
target is not met by this proxy; the latest owner gate compares against QML.

`bb054fa2` keeps GL through an active pointer visual and release handoff;
`ef9d5c01` refreshes its idle clock on slider input. Closed panels still expire.
The valid native AMD GL arm stays on GL throughout all twelve gestures and has
p50/p95/p99 10.491/11.921/12.063 ms at 120 Hz. CPU is cheaper in this comparison,
so controls retain their existing CPU default. No Intel or fresh GL60 result is
claimed. [GL result and hashes](evidence/0126/qml-parity-gl-latency.json).

Five alternating CPU pairs use the same integrated source with only the three
CPU drag improvements removed as control; both include the newer unrelated main
changes and GL lifecycle fixes. Median candidate−control unreclaimable memory
(anon+SHM+DRM+SwapPss) is idle −0.055, open +0.156, post-drag −0.867 and closed
+0.270 MiB: all pass +0.5 MiB and stay within the declared 1 MiB feature budget.
This memory harness uses the existing nested 60 Hz niri under a 120 Hz parent;
these memory results are separate from direct-path timing. File-backed PSS is
informational. Host swap-in and process SwapPss are zero. Fresh fixture first-key
CPU passes 8 ms: 6.921 ms control / 7.110 ms final, with exact typed state and
64 result rows from 320 fixture applications.
[Every pair, components, source hashes and key proof](evidence/0126/qml-parity-memory.json).

Separate visual arms verify home 41/41 and Sound 43/43 cursor-free samples for
both Bead and QML. Full-window images can include the idle pointer outside the
slider ROI. All 181 home and 203 Sound native shared paints have exactly
zero fill-end/handle-center difference. Local label tests and runtime text bakes
cover changes before release; the Sound pressed image shows the updated 85%
status. [Per-paint and per-image results](evidence/0126/qml-parity-coherence.json).

| State | Home Bead / QML | Sound Bead / QML |
| --- | --- | --- |
| idle | [bead](evidence/0126/qml-parity-home-bead-idle.png) / [qml](evidence/0126/qml-parity-home-qml-idle.png) | [bead](evidence/0126/qml-parity-sound-bead-idle.png) / [qml](evidence/0126/qml-parity-sound-qml-idle.png) |
| open | [bead](evidence/0126/qml-parity-home-bead-open.png) / [qml](evidence/0126/qml-parity-home-qml-open.png) | [bead](evidence/0126/qml-parity-sound-bead-open.png) / [qml](evidence/0126/qml-parity-sound-qml-open.png) |
| hover | [bead](evidence/0126/qml-parity-home-bead-hover.png) / [qml](evidence/0126/qml-parity-home-qml-hover.png) | [bead](evidence/0126/qml-parity-sound-bead-hover.png) / [qml](evidence/0126/qml-parity-sound-qml-hover.png) |
| pressed | [bead](evidence/0126/qml-parity-home-bead-pressed.png) / [qml](evidence/0126/qml-parity-home-qml-pressed.png) | [bead](evidence/0126/qml-parity-sound-bead-pressed.png) / [qml](evidence/0126/qml-parity-sound-qml-pressed.png) |
| after-drags | [bead](evidence/0126/qml-parity-home-bead-after-drags.png) / [qml](evidence/0126/qml-parity-home-qml-after-drags.png) | [bead](evidence/0126/qml-parity-sound-bead-after-drags.png) / [qml](evidence/0126/qml-parity-sound-qml-after-drags.png) |

Active audio slider placement, values and coherent paint agree. Existing wider
parity gaps remain visible: Bead omits the Home VPN tile, unavailable control
states/row packing and notification wording differ, and bar provider counts
vary; QML adds its own Sound peak-detector stream and the Sound header alignment
is slightly different. Complete all-page parity, general dragging, strictly
lower cursor pixel error and physical/Intel acceptance remain unproven.
[Full CI, source provenance and excluded attempts](evidence/0126/checks.md).

## Continuation: dispatch

Established slider motions are handled in the Wayland protocol dispatch, before
SCTK waits for `wl_pointer.frame`; SCTK still owns button/axis ordering and seat
serials. Non-motion events are ordering barriers. Only the latest stored pose is
submitted after the ready fd drains, before the event loop sleeps; no motion timer
or next-frame callback delays input dispatch. The existing single outstanding
parent callback still paces commits. Raw-wire/handler clocks distinguish client
queueing from the upstream nested input path. Full Arch CI passes. A fresh matched
CPU pair gives total p50/p95 32.018/35.863 ms before and 32.042/35.869 ms after;
no measurable end-to-end gain. Wire→handler p95 is 0.006 ms (372 motions).
The parent is 120 Hz while nested niri still advertises 60 Hz; this does not
establish physical 120 Hz performance. The new 25 ms gate remains open.
[Exact joined timing records and source hashes](evidence/0126/dispatch-results.json).

## Continuation: shared slider paint

The toolkit exports one stable slider scope, its fill style and its travel origin.
Fill and handle are painted from the same pose into one synchronized SHM child;
only its 16 px-high damage region is repainted. Two reusable slots are bounded
at 128 KiB total. No font system or panel scene is constructed for this paint.
Clip/occlusion changes safely restore ordinary rendering. Release uses an accepted
transaction serial, preserving the final pending value before restoring the parent.
Pixel tests cover 101 poses at scales 1 and 1.5; descriptor origins also pass at
1, 1.5 and 2. Full Arch CI passes; presented-frame and memory results follow.
The child uses a continuous linear gradient like QML; idle kit Fill retains its
stepped approximation, a known color-interpolation difference on handover.

## Continuation: local values without panel work

`Session::local_range_feedback` commits the range value and an authored numeric
or prefix/precision/suffix argument without running the view or layout. Control
and sound templates supply that argument format; unsupported actions retain the
ordinary path. Native paints flush the latest local value once per committed
slider frame. Only service writes retain the 34 ms cap, with a final release flush.
Service sources already own incoming data: empty update notifications skip view
projection while held, and release projects the newest snapshot. Explicit model
changes and close commands remain immediate. Call tickets and newest queued
service targets drain independently of projection, including while held.
Tests observe unchanged nodes/fill bounds and zero projector calls for local
values, resumed projection on release, responsive close, and fake-daemon writes.
The full Arch CI passes, including runtime projection and fake-daemon transport
regressions. The shipped product commits are `312c63e7`, `c7772734`, `e2d95769`
and `3731c52e`; each passed the full remote Arch job before safe-push.
Ancillary status/percentage text in sound cards remains at its accepted scene
value while held and refreshes on release; only fill and handle receive local
paint updates. This is a remaining QML interaction-parity gap.

### Fresh continuation measurements

Five alternating matched CPU pairs have 345 control and 395 final exact
coordinate/clock joins at 2000 physical px/s. The parent is 120 Hz; nested niri
advertises 60 Hz. These are presentation-feedback proxies, not physical scanout.
The 25 ms total p95 gate **fails** on this nested path.

| Stage, p50 / p95 ms | Control | Final |
| --- | ---: | ---: |
| Injection → dispatch | 10.026 / 11.891 | 9.994 / 11.635 |
| Dispatch → commit | 11.431 / 12.155 | 0.052 / 11.888 |
| Commit → feedback | 12.448 / 15.975 | 15.909 / 15.964 |
| Total | 32.033 / 35.864 | 26.499 / 33.217 |

The separate cursor-free coherence capture has 82 joins, total p50/p95
26.500/33.469 ms. In that capture,
The 70 remaining poses have dispatch→commit p50/p95 0.052/0.081 ms;
the 12 first poses have 11.667/11.935 ms. The first recorded pose of each gesture still
pays for mapping the child and masking the parent atomically. The corresponding
parent render takes 11.25–11.87 ms after an immediately received callback.
The nested compositor contributes about 16 ms after commit. Removing the callback
guard would allow multiple frames in flight and is not an accepted optimization.

Fill and handle agree in **41/41 sampled frames**, versus 30/41 before shared
paint and 41/41 in QML. All **106 shared paints** use exactly the same fill edge
and handle-center coordinate. This establishes committed-pose identity; the PNG
samples do not enumerate every physical scanout frame. Runtime tests separately
prove local value updates leave the accepted nodes/layout unchanged and do not
invoke projection. The capture records 12 distinct post-store-commit local
positions and shared paint positions; scene-only tracing sees just two endpoint
updates because value changes no longer publish whole scenes.

[Exact timing joins](evidence/0126/final-latency.json),
[five-pair matched timing joins](evidence/0126/nested-cohort-latency.json),
[first-pose pacing brackets](evidence/0126/pacing-summary.json),
[exact shared-pose records](evidence/0126/final-shared-pose.json), and
[sampled frame analysis](evidence/0126/final-fill-coherence.json) retain the data.
[Source hashes, output geometry and safety results](evidence/0126/continuation-provenance.json)
pin the nested final and matched direct control/candidate arms.
Representative cursor-free drag crops: [Bead](evidence/0126/continuation-bead-drag.png)
and [QML](evidence/0126/continuation-qml-drag.png). They show separate sampled
poses, so they are a continuity check rather than a same-coordinate pixel golden.
Strict moving pixel-error superiority over QML remains unproven.

### Direct private 120 Hz proxy

To separate the nested input/presentation hops, the existing private labwc
headless parent also hosts the UI directly. Its queried mode is 2880×1800 at
120 Hz, scale 1.5, logical 1920×1200. Bead uses `--backend none`; the existing
private niri socket remains solely for the read-only helper adapter. QML uses
the same direct output. All swap, source, containment, watchdog, geometry and
full-window capture gates remain enabled. This reuses the private compositor;
it is a synthetic presentation-feedback proxy, not a physical/niri-output result.

The final CPU candidate **passes p95 ≤25 ms on this proxy**: total p50/p95 is
10.548/17.291 ms (146 exact joins, 12 gestures). Input→dispatch p95 is 0.044 ms,
dispatch→commit 12.163 ms, and commit→feedback 8.052 ms. Total p99 is 125.262 ms;
outliers are retained, so complete smoothness is not claimed. Sampled coherence
is 41/41, and all 181 shared paints have zero fill-edge/handle-center difference.
The matched direct control has total p50/p95 16.447/18.779 ms (91 joins), with
30/41 coherent samples. The candidate presents more distinct coalesced poses
while reducing latency. QML has lower p95 on this direct path. All timing
outliers are retained; a physical 120 Hz niri result is unmeasured.

| Direct CPU proxy | Control | Final | QML |
| --- | ---: | ---: | ---: |
| Exact presented-pose joins | 91 | 146 | 180 |
| Total p50 ms | 16.447 | 10.548 | 10.326 |
| Total p95 ms | 18.779 | 17.291 | 11.979 |
| Total p99 ms | 154.303 | 125.262 | 143.587 |
| Coherent sampled frames | 30/41 | 41/41 | 41/41 |

QML's first probe could not observe its slider state through the legacy mouse
grabber API, so that arm supplies no latency number. A measurement-only
[probe patch](evidence/0126/qml-state-probe.patch) caches the unique control under
each press and reads its actual values at synchronization. It does not mutate
the UI or relax the frame/generation/state/presentation join. The replacement
has 180 accepted joins, zero generation-contract misses, zero rejected positions
and zero unmatched coordinates. Its sampled pixel check is independently 41/41.
[Exact proxy joins](evidence/0126/direct-candidate-latency.json),
[matched control joins](evidence/0126/direct-control-latency.json),
[state-verified QML joins](evidence/0126/direct-qml-state-latency.json),
[sampled coherence](evidence/0126/direct-candidate-fill-coherence.json),
[shared paint records](evidence/0126/direct-candidate-shared-pose.json).

### Fresh memory and typing checks

The same five alternating CPU pairs pass the +0.5 MiB median-per-phase regression
guard against the default-thumb/readiness control. All capture inputs are stable
within each phase; host swap-in and process SwapPss are zero in every arm.
Only anonymous + SHM + DRM + SwapPss are gated. These are desktop-relative
measurements, not an absolute laptop-cap reading.

| Phase | Paired median Δ unreclaimable MiB | Δ file-backed PSS, info |
| --- | ---: | ---: |
| Idle | −0.055 | +0.108 |
| Open | −0.301 | +0.104 |
| Open after drags | −0.324 | +0.104 |
| Closed, 20 s | −0.168 | +0.097 |

The differences are anonymous residency; DRM/SwapPss stay zero and SHM has no
median increase. File-backed PSS is informational. The original accepted thumb
feature's peak median costs, +0.711 MiB CPU / +0.914 MiB pinned GL, remain within
its declared 1 MiB feature budget; this continuation introduces no measured
CPU relative regression. Fresh GL continuation memory is unmeasured; its feature
configuration passes the full CI checks.

Fresh fixture first-key process CPU is **7.102 ms control / 7.070 ms final**, both
below 8 ms. Both use 320 fixture apps, 12 favorites, 8 recents and 64 result rows;
one real key is matched to the exact typed state and generation/update marker.
The initial combined drag/typing arm failed before measurement because the CLI
rejects fixture mode with the production UI provider. The corrected run reuses
the existing local-command-socket fixture benchmark, preserving all capture
guards and source checks. It is UI CPU timing, not real-engine or physical
presentation acceptance. [Five-pair components, source hashes and fresh typing
results](evidence/0126/continuation-memory.json) retain the measurements.

## Experiment (historical thumb-only baseline)

The normal renderer already submits on input when its frame callback is free.
Removing that pacing guard repeats B0105's rejected multiple-frame experiment.
This candidate instead moves a synchronized `wl_subsurface` with one reusable,
CPU-rastered thumb buffer (≤64 KiB), an empty input region and a scale-correct
viewport. A parent state-only commit needs neither panel raster nor text work;
it uses the existing single outstanding frame callback. Mapping and unmapping
are staged with the parent repaint to preserve the thumb across handover.

Luau supplies the thumb ID, pointer bounds and travel range through kit Slider
metadata; the engine contains no service IDs. Absolute positions are clamped
without accumulating deltas. Worker state updates are coalesced at 34 ms; the
last update precedes release. Thus pointer poses avoid daemon/Luau/layout work,
but the coalesced fill/label updates still evaluate the panel. The fill was not
viewport-stretched because that would distort its rounded gradient geometry.
Only standalone rounded horizontal layers participate; arbitrary draggables,
clipping/occlusion and animated panels are not certified by this experiment.

The patch includes two diagnostic pins: explicit renderer selection avoids the
bar's initial lazy policy, and `BEAD_EXPERIMENTAL_POINTER_PIN_GL=1` selects GL
when opening a dynamic panel, whose separate policy otherwise overrides it.
GL control and candidate share both changes; CPU measurements precede the second
pin, which is inactive without that environment variable. Both diagnostic pins
are withdrawn with the prototype; normal default `auto` behavior is preserved.

## Measurement method and limits

All UI work ran in private nested labwc→niri sessions at 2880×1800, scale 1.5,
with the same palette, settings, frozen helper and QML source `836df1a643ea`.
The GPU is an AMD integrated/radeonsi proxy; Intel is unmeasured. Parent 60/120 Hz
modes are synthetic scheduling, not physical scanout measurements. A private
session on the daily output would exceed this unit's no-live-desktop boundary.
Both arms start from Bead `61e73507`; later unrelated main changes are excluded
from the matched experiment. All arms require zero host swap-in/SwapPss and
host swap below 64 MiB before starting; no swap reset was needed.

The private actuator flushes absolute motions at 250 Hz with monotonic deadlines
and stationary endpoints. Actual speed must be 1950–2050 px/s. Twelve alternating
gestures supply trace samples; one gesture has lossless cursor-inclusive ROIs.
Trace joins use exact injected coordinates and matching state/commit/presentation
identities; stationary endpoints calibrate coordinate spaces independently.
Percentiles use nearest rank. Coalesced inputs without a presented state are not
latency samples; discarded feedback is not instrumented, so no all-input deadline
or dropped-frame guarantee is claimed. Pixel analysis uses cursor/knob pixels
in the same image, validates its frame manifest and requires eight distinct pairs
spanning half the trajectory. Screenshot spawn times are never presentation times.

Earlier attempts are excluded: a private input-proof flag intercepted slider
presses; per-motion roundtrips slowed injection; cursor-less images could not
measure error; the first prototype selected an invisible hit rectangle instead
of the painted primitive; and old GL arms could switch to CPU. The retained
patch selects the actual painted scene layer and tests visible BGRA pixels.

## Timing results

Values are p50 / p95 ms; each stage has its own distribution, so columns do not
sum. [Numeric summaries and source hashes](evidence/0126/results.json);
[raw join records and memory snapshots](evidence/0126/trace-samples.jsonl).

| CPU stage | Control 60 | Candidate 60 | Control 120 | Candidate 120 |
| --- | ---: | ---: | ---: | ---: |
| Injection → dispatch | 18.78 / 28.78 | 18.28 / 24.79 | 10.28 / 14.75 | 10.01 / 11.89 |
| Dispatch → worker | 4.82 / 6.01 | — | 1.46 / 8.02 | — |
| Worker → layout | 4.89 / 6.29 | — | 4.92 / 5.84 | — |
| Layout → paint ready | 0.24 / 0.26 | — | 0.23 / 0.26 | — |
| Paint ready → accepted | 0.49 / 0.65 | — | 0.46 / 0.67 | — |
| Accepted → commit | 5.95 / 20.26 | — | 3.08 / 9.82 | — |
| Dispatch → pose commit | — | 12.08 / 27.71 | — | 11.27 / 12.61 |
| Commit → feedback | 31.27 / 31.50 | 19.86 / 31.97 | 15.31 / 15.54 | 11.95 / 15.99 |
| Total injection → feedback | 65.76 / 72.86 | 50.23 / 56.77 | 37.57 / 43.62 | 32.02 / 35.87 |
| Presented samples | 63 | 60 | 104 | 69 |

QML totals are 65.04 / 79.25 ms at 60 Hz (76 presented generations) and
33.35 / 50.91 ms at 120 Hz (104). QML p99 is 98.57 / 164.76 ms respectively;
the largest tails occur in the screenshot-sampled first gesture, which may
perturb the path; these outliers are retained.
The CPU candidate p99 is 71.01 / 94.18 ms. Both candidate p95 values fail the
21 / 12 ms gates; faster feedback than QML does not establish better pixel error.

Pinned native GL uses the AMD proxy and preserves GL transport throughout the
arm; neither CPU fallback nor idle GL→CPU transition is accepted. Values below
are p50 / p95 ms; the full worker/layout/paint stages are in the numeric evidence.

| GL stage | Control 60 | Candidate 60 | Control 120 | Candidate 120 |
| --- | ---: | ---: | ---: | ---: |
| Injection → dispatch | 17.75 / 28.69 | 18.03 / 22.81 | 10.27 / 14.83 | 10.02 / 14.81 |
| Dispatch → pose commit | — | 11.51 / 27.51 | — | 11.53 / 19.59 |
| Commit → feedback | 31.57 / 31.65 | 20.44 / 31.97 | 15.61 / 17.64 | 12.46 / 15.99 |
| Total injection → feedback | 65.73 / 70.80 | 49.98 / 54.83 | 38.71 / 46.83 | 32.90 / 37.18 |
| Presented samples | 61 | 60 | 105 | 80 |
| Total p99 | 126.98 | 71.65 | 157.42 | 269.33 |

GL misses both gates. Its small p95 advantage at 60 Hz reverses at 120 Hz;
these single timing arms establish no decisive GL advantage over CPU. The
[GL candidate frame](evidence/0126/gl-candidate-drag.png) also shows the fill
trailing the moved thumb. Intel and a battery-powered laptop remain unmeasured.

[One fixture launcher first-key sample](evidence/0126/first-key.json) is 6.981 ms control / 7.415 ms candidate
UI process CPU, both below 8 ms; input→feedback is 7.273 / 7.693 ms. The fixture
has 320 apps, 12 favorites, 8 recents and 64 result rows. This is actual typed
state joined to its commit/feedback, not end-to-end daemon typing acceptance.

Pixel error is **unproven**: the cursor-inclusive ROIs have too few stable moving
cursor/knob pairs for the required coverage. No numeric p95 or strict QML win is
claimed. These [control](evidence/0126/cpu-control-drag.png) and
[candidate](evidence/0126/cpu-candidate-drag.png) and
[QML](evidence/0126/qml-drag.png) frames illustrate the visible
thumb in separate gestures; they are not time-aligned latency measurements.
The current Bead hand cursor also differs from QML's arrow. The candidate's
coalesced fill visibly trails its thumb. Complete pressed/hover,
all-page parity, smoothness and dropped-frame certification remain open.

## Memory

Shipping-code full CI passes;
[commands, source revisions and log hashes](evidence/0126/checks.md) distinguish
validation of the experiment from validation of the unchanged product code.

Five alternating CPU pairs use the same binary flags, output and fixture, with
5 s idle/open/post-drag settles and 20 s closed settle. UI process totals exclude
the helper/daemon. [All components and five pair deltas](evidence/0126/memory-cpu.json).
Absolute medians and medians of paired differences need not subtract exactly.

| CPU phase, MiB | Control unreclaimable | Candidate unreclaimable | Paired Δ | PSS control/candidate | File PSS control/candidate |
| --- | ---: | ---: | ---: | ---: | ---: |
| Idle | 19.951 | 19.939 | −0.023 | 39.975 / 40.034 | 20.023 / 20.100 |
| Open | 35.086 | 34.887 | −0.082 | 56.098 / 55.876 | 20.915 / 20.987 |
| Post-drag | 39.756 | 40.279 | +0.711 | 60.567 / 61.327 | 20.953 / 21.018 |
| Closed | 33.387 | 33.801 | +0.414 | 54.456 / 54.774 | 20.961 / 21.042 |

The largest paired-median CPU cost, +0.711 MiB after dragging, fits the declared
1 MiB feature budget. Its median is entirely anonymous; paired-median SHM, DRM
and SwapPss deltas are zero there and after close. The retained anonymous cost is
not byte-attributed to individual allocations, so this is not an allocator fix
or memory optimization claim. Two post-drag pairs exceed +1 MiB (+1.043/+1.707);
all five remain included. PSS and file-backed PSS are informational, not gated;
DRM is zero for CPU arms, so PSS+DRM equals the reported PSS. This proxy does not
establish the laptop's absolute cap.

Five alternating pinned-GL pairs use the same settle windows. The bar is CPU;
the opened panel is native GL, and the diagnostic pin retains GL resources into
the closed phase. [Components and every pair](evidence/0126/memory-gl.json).

| GL-panel phase, MiB | Control unreclaimable | Candidate unreclaimable | Paired Δ |
| --- | ---: | ---: | ---: |
| Idle (CPU bar) | 20.029 | 20.029 | −0.094 |
| Open | 150.073 | 146.487 | −3.977 |
| Post-drag | 121.769 | 122.687 | +0.914 |
| Closed, retained GL resources | 157.001 | 156.907 | +0.109 |

| GL-panel accounting, control / candidate MiB | Idle | Open | Post-drag | Closed |
| --- | ---: | ---: | ---: | ---: |
| DRM total | 0 / 0 | 95.199 / 91.746 | 64.590 / 64.590 | 98.961 / 98.961 |
| PSS | 40.039 / 40.111 | 100.321 / 100.370 | 102.712 / 103.753 | 103.623 / 103.694 |
| PSS + DRM | 40.039 / 40.111 | 195.731 / 192.116 | 167.302 / 168.343 | 202.592 / 202.605 |
| File PSS (information) | 19.974 / 20.074 | 45.526 / 45.555 | 45.554 / 45.580 | 45.590 / 45.616 |

The maximum paired-median GL feature cost, +0.914 MiB, is inside the 1 MiB
budget, with little margin. Its anonymous median is +0.926 MiB, SHM/SwapPss
are unchanged, and small DRM differences produce the total. Two post-drag pairs
exceed +1 MiB; all remain included. Open-phase DRM differences span roughly
±7.8 MiB, so the negative open median is not an optimization claim. The pin's
large absolute retained GL footprint is common to both arms and is not an
accepted always-on policy. Relative-budget results do not establish Intel,
laptop-cap, smoothness or feature acceptance. Every arm has zero host swap-in
and process SwapPss; captures and source/renderer guards passed.

## Verdict and follow-up

The earlier claim that Nacre volume/mic sliders share an action key was wrong:
their keys already include the source-specific first/last action arguments.
Generic instances can still reuse a key; descriptor matching now also checks
the authored slider scope. Fill/handle identity and clipped/stacked handling
remain part of the continuation. The owner accepted the measured candidate
despite the old 21/12 ms gates; the new 25 ms gate applies to subsequent work.
The task does not authorize changing the daily desktop to obtain physical-output
measurements. No installed software, configs, daemon, widgets, picker or lips
were changed in the initial investigation. The default-on continuation changes
framework pointer visuals and kit metadata, preserving existing GL policies.
