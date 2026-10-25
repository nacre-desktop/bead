# B0126 — instant slider drag

Status: owner accepted the measured candidate on 2026-10-25; the measured thumb path is now enabled by default. The new continuation gate is 120 Hz total p95 ≤25 ms with fill and
handle derived from the same value in every frame. Improvements proceed in order:
immediate fd dispatch, shared slider pose, then slider-only updates. The prior
21/12 ms rejection below is historical, superseded by this explicit acceptance.

The preserved
[patch](evidence/0126/compositor-knob.rejected.patch) applies to `61e73507`.
It records the measured implementation; the product version omits diagnostic renderer pins.

The historical gates were pointer→present p95 ≤21 ms at 60 Hz and ≤12 ms at 120 Hz,
moving pixel error at 2000 physical px/s strictly below QML, first-key UI CPU
≤8 ms, and ≤1 MiB added unreclaimable memory. Unreclaimable means anonymous
+ SHM + DRM + SwapPss. File-backed PSS is informational. Five alternating
control/candidate pairs are required for memory acceptance.

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

## Experiment

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
