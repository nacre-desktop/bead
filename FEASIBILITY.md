# Feasibility

## B0147 — Spotlight overlay (2026-10-27; smoothness gates fail)

Spotlight now floats directly over the desktop; Luau owns layout and the carousel motion.
An idle worker warms the real thumbnails and passes a prepared CPU scene to the lazy panel while GL initializes.
Two real-wallpaper closed-retention arms stay within B0134's 40 MiB budget.
The paired QML/Bead difference is the requested removal of QML's full-screen selected-wallpaper backdrop.
Opening and slide cadence miss synthetic 60/120 Hz frame limits (not physical scanout), and a slide probe exceeds Luau's 50 ms markup limit. 4K Enter hits the separate cache-admission path.

| 2880×1800, scale 1.5 | 60 Hz | 120 Hz | Gate |
| --- | ---: | ---: | --- |
| First real-4K panel frame after trigger | 92.1 ms | 93.0 ms | fallback content shown |
| Expired-closed unreclaimable delta | 37.05 MiB | 36.36 MiB | ≤40 MiB, 2 samples pass |
| Cold-open p99 interval / gaps >1.5× vsync | 56.79 ms / 2 | 5.69 ms / 1 | no dropped frames, fail |
| Expired-open p99 interval / gaps >1.5× vsync | 53.09 ms / 4 | 62.11 ms / 3 | no dropped frames, fail |
| Spotlight slide p99 interval / gaps >12.5 ms | — | 22.73 ms / 2 | frame-rate motion, fail (3 frames / 400 ms) |
| Large raster LRU charge / limit | 14.64 / 22 MiB | 14.64 / 22 MiB | bounded |

The 4K activation error is recorded without changing B0146/B0148 switch code. Full Bead CI and regenerated goldens pass; no live deployment. [Scope, paired images and run evidence](docs/specs/0147-spotlight-overlay.md).

## B0149 — smooth held-key OSD (2026-10-26, shipped)

Nacre publishes optimistic volume and brightness steps; Bead retargets the pill fill without queuing animation steps.
All 16 changed volume values committed within one synthetic 60 Hz frame; brightness commits were usually faster than one frame.
The first cross-kind brightness shadow raster took 131.6 ms and two near-zero updates took 19.3 / 25.1 ms; the owner approved shipping against the non-notifying legacy `brightnessctl` path.
Full remote desktop CI passed in both repositories; this capture used synthetic key-action replay and did not measure physical scanout.

| Bead overlay commit after daemon publish | Changed values | p50 | Maximum | Result |
| --- | ---: | ---: | ---: | --- |
| Volume | 16 | 10.8 ms | 12.8 ms | 16/16 within 16.7 ms |
| Brightness | 20 | 8.3 ms | 131.6 ms | 17/20 within 16.7 ms; follow-up |

[Scope, paired images and samples](docs/specs/0149-osd-smooth.md). Retain the shadow-raster outlier as a renderer follow-up.

## B0141 — scrolling follow-up (2026-10-26, in progress)

The historical allocator abort comes from the standalone Luau analyzer's parallel reporter race.
The UI check exits successfully; updating the pinned analyzer fixes the reproduced tool fault.
Active-frame cache retirement passes renderer tests while preserving pixels and scale reuse.
Picker40MiB, AI one-frame, open-view budgets and the30-minute runtime soak still need native validation.

| Remote exact analyzer checks | Old0.740 | Fixed0.742 |
| --- | ---: | ---: |
| Original tcache-message reproduction | run12 abort | 100/100 pass |
| Explicit glibc debug allocator preload | run28 SIGSEGV | 100/100 pass |
| Renderer active-window tests | — | 70/70 pass |

The analyzer/runtime attribution corrects B0135's earlier assumption. No runtime Luau upgrade.
[Failure evidence and remaining gates](docs/specs/0141-scrolling-followup.md).

## B0135 — shared scrolling (2026-10-26)

One source-aware policy replaces amplified finger input and queued wheel reversal.
Virtual lists, retained poses and viewport damage reduce repeated CPU work.
Real finger-stop motion and cancellation pass; most latency/cadence targets still fail.
Full remote CI passes; five-pair CPU memory fails history/Clipboard open budgets.

| Matched private probes, scale 1.5 | Before | Candidate |
| --- | ---: | ---: |
| AI transcript finger render p95 / reversal, ms | stuck at bottom | 2.09 / 49.22 (reversal fails) |
| Settings side-tab wheel render p95, 3 repeats ms | 15.59–16.15 | 1.29–1.51 |
| Rapid notification wheel reversal, 3 repeats ms | 63.34–63.91 | 19.08–19.56 (fails) |
| Direct-Labwc finger-stop reversal, ms | 16.41 | 11.35 (fails) |
| History / Clipboard open median unreclaimable Δ, MiB | — | +1.484 / +2.176 (both fail 1 MiB) |
| Eight-image picker open median unreclaimable Δ, MiB | — | −36.606 |
| GL80 finger / reopened wheel render p95, ms | setup fails | 0.86 / 0.74 |
| GL80 reversal finger / wheel, ms | unavailable | 281.93 / 559.72 (fails) |

Demand-driven global admission grows 192→256 MiB to eliminate a measured reopen
failure. GL80 sampled process unreclaimable memory is 38.47 MiB idle and
481.52 / 528.64 MiB during finger/wheel, including DRM; this is a substantial
stress cost, not the laptop cap or final CPU memory cohort. Historical shared-target
control artifacts have ambiguous provenance and do not supply final gates.
No physical scanout or zero-drop acceptance. Long clamped gaps remain reported.
[Source-pinned results, failures and parity gaps](docs/specs/0135-scrolling.md).

## 2026-10-26 — B0134 bounded picker and video handoff

A compressed CPU first frame keeps reopened content populated while GL prepares.
Closed textures, surfaces and exclusively owned driver pools retire after two seconds.
Five alternating pairs pass retention, relative memory, cold and static-selection gates.
Video-picker palette and expired warm-presentation feedback still miss their targets.

| CPU bar / GL picker, five-pair median | Result |
| --- | ---: |
| Largest static closed retention above idle | 28.868 MiB ≤ 40 |
| Static wallpaper/bar; picker palette | 124.341; 127.925 ms |
| Video wallpaper/bar; picker palette | 147.363; **150.743 ms** |
| Expired warm draw; presentation feedback | 12.401; **17.956 ms** |
| GL carousel render p99, separate proof | 1.141 ms |

Full remote and official Bead/Nacre CI pass. Three video-picker samples fail 150 ms.
Clean file PSS is informational; no physical scanout or laptop-cap acceptance.
[Source pins, all samples and remaining limits](docs/specs/0134-picker-retention.md).

## 2026-10-26 — B0129 launcher first-frame icons

The launcher retains a bounded shared SVG/PNG raster set after its panel closes.
It primes the first real Panel query and shares only immutable embedded Luau compiler bytes.
Five alternating private CPU pairs remove missing icons on cold, warm and actual 120-second expiry opens.
The measured feature cost exceeds the preferred 1 MiB but fits this unit’s allowed 4 MiB smoothness budget.

| Median candidate − control | Cold | Warm | Expired |
| --- | ---: | ---: | ---: |
| First presented, ms | -1.040 | +0.081 | +0.713 |
| Candidate missing-icon presentations | 0 | 0 | 0 |

First-key echo UI CPU max is 3.928 ms; largest phase median unreclaimable delta
is +2.895 MiB (warm_closed). File PSS is informational; UI-only numbers exclude daemon/providers.
Full remote Arch CI passes; matched QML/Bead state captures expose the listed visual gaps.
[All samples, spreads, rejected candidate and limits](docs/specs/0129-launcher-icons.md).
Final search-results UI CPU remains 10.917 ms median; the 8 ms gate above measures the first echo.
Startup excursions exceed 4 MiB; acceptance uses five-pair phase medians, not peak residency.
No live/laptop-cap or physical scanout acceptance is claimed.

## 2026-10-26 — B0126 QML-relative drag gates

The first drag rebuilt a frame image and 59 text layers; cached rasters cut its masked frame from 12.095 to 0.562 ms.
Local labels avoid layout, and unchanged placement no longer emits redundant parent commits.
Three alternating pairs per rate pass the latest QML-relative p95 and two-frame p99 gates.
Shared fill/handle paints and separate home/Sound visual samples agree; controls retain the cheaper CPU default.
All five source steps passed full remote Arch and exact-commit GitHub CI.

| Direct private CPU proxy | Bead p95 / p99 | QML p95 | Gate |
| --- | ---: | ---: | --- |
| 60 Hz, 3 pairs | 26.271 / 26.517 ms | 26.801 ms | pass |
| 120 Hz, 3 pairs | 11.764 / 11.875 ms | 11.823 ms | pass |
| Native AMD GL120 | 11.921 / 12.063 ms | 11.823 ms | CPU preferred |

Five-pair unreclaimable deltas idle/open/post-drag/closed are −0.055/+0.156/−0.867/+0.270 MiB (all ≤+0.5 MiB).
Fixture first-key CPU is 7.110 ms; native home/Sound poses agree 384/384 and sampled frames 84/84 per implementation.
Timing uses queried 2880×1800 scale 1.5 synthetic labwc feedback, not physical niri scanout.
Intel/laptop latency, absolute laptop memory, general cursor-error superiority and complete page parity remain unmeasured.
The later-main native-size first-key regression is also fixed by `27abdf62`:
idle frame credit primes the existing buffer before input; median key CPU 9.149→7.346 ms (max 7.374).
Fresh CPU60/120 p95 26.274/11.764 ms and p99 26.382/11.939 ms pass the same QML cohort.
Five extra CPU memory pairs pass; largest phase median +0.080 MiB, open −2.514 MiB (mostly SHM).
Another 182 native paints and 41 ROI images are coherent. Full remote Arch CI passes.
[All samples, earlier regressions, source provenance and paired images](docs/specs/0126-instant-drag.md).

## 2026-10-26 — B0126 shared slider paint and local values

Fill and handle now share one value and one small buffer, preventing independent lag.
The paint borrows SHM directly and avoids constructing a font system or panel scene.
Full Arch CI passes, including local values without projection and service call progress.
Shared paint stays coherent; local values avoid projection/layout while service calls progress.
The CPU memory guard passes; 25 ms p95 passes on the direct private 120 Hz proxy and fails nested.

| Check | Result |
| --- | ---: |
| Shared-paint pixel poses | 101 at 1× and 1.5× |
| Descriptor origin scales | 1× / 1.5× / 2× |
| Child SHM bound | ≤128 KiB, two slots |
| Coherent sampled frames, old / shared / QML | 30/41 / 41/41 / 41/41 |
| Shared paint value differences | 0 px across 94 paints |
| Dispatch-only CPU p95 before / after | 35.863 / 35.869 ms |
| Five-pair nested CPU total p50 / p95 | 26.499 / 33.217 ms |
| Steady dispatch→commit p95, 70 poses | 0.081 ms |
| First-pose dispatch→commit p95, 12 poses | 11.935 ms |
| Final coherent frames / shared poses | 41/41 / 106/106 |
| Direct private 120 Hz CPU total p50 / p95 | 10.548 / 17.291 ms |
| Direct QML CPU total p50 / p95 | 10.326 / 11.979 ms |
| Five-pair CPU Δ idle/open/post-drag/closed MiB | −0.055/−0.301/−0.324/−0.168 |
| Fresh fixture first-key CPU, control / final | 7.102 / 7.070 ms |

Nested niri advertises 60 Hz under a synthetic 120 Hz parent; physical scanout is unmeasured.
The same private headless parent hosting the UI directly passes 25 ms p95; this is not niri acceptance.
QML has lower p95 on that direct proxy; first-key CPU passes 8 ms.
[Timing joins, coherence and remaining handover cost](docs/specs/0126-instant-drag.md#fresh-continuation-measurements).

## 2026-10-25 — B0123 Spotlight retention and palette

Spotlight now floats over the desktop and waits for real thumbnails before publishing.
Prepared GL retention removes the expired blank open and visible promotion hitch.
The never-opened budget passes; warm GPU residency is a large explicit smoothness spend.
Nested clocks are not physical scanout; CPU fallback and laptop-cap limits remain.

| CPU bar / GL picker | Result |
| --- | ---: |
| First-ever / warm / 125 s first draw | 224.466 / 10.022 / 9.632 ms |
| Warm / expired render p99 | 2.621 / 1.933 ms |
| Five-pair idle unreclaimable delta | +0.266 MiB |
| Five-pair expired unreclaimable delta | +303.539 MiB |
| Expired DRM component delta | +263.152 MiB |

CPU rounded-paint p99 remains 38–44 ms. First-ever hidden GPU preparation is 95.719 ms.
[Paired evidence, synchronized palette clocks and limitations](docs/specs/0123-picker-spotlight-palette.md).

## 2026-10-25 — B0114 widget editor feature-budget decision

The lead accepted the editor’s measured cost under a 2 MiB always-on feature budget.
The original five-pair CPU measurements fit that budget; the regression guard remains separate.
Earlier Arch CI and nested save/dropdown/focus proofs pass; fresh integration and drag checks continue.
The laptop absolute gate was not run.

| Median candidate − control, MiB | Idle | Launcher | Typed | Closed + 12 s |
| --- | ---: | ---: | ---: | ---: |
| Unreclaimable | +0.555 | +0.820 | +0.754 | +0.723 |
| Clean file PSS, informational | +0.203 | +0.236 | +0.237 | +0.237 |

Growth is anonymous; SHM is identical and DRM/SwapPss are zero. All five pairs remain.
First-key UI CPU max is 4.067 ms; nested first response max is 6.042 ms, not scanout.
[Candidate, full samples, failed parity control and unfinished gates](docs/specs/0113-widget-system.md#phase-2-candidate--b0114-staged-integration).

## 2026-10-25 — B0119 keyboard focus and title churn

A closed warm panel could reclaim exclusive keyboard focus on its next update.
Drawing now owns keyboard policy; close releases input and the keyboard immediately.
Bound text measurements keep equal-width title changes out of the markup VM and layout.
Matched private niri sessions use 2880×1800 / 1.5; no live desktop changes.

| Run | Keyboard delivery | Shell CPU, % core | Niri CPU, % core |
| --- | ---: | ---: | ---: |
| Control (protocol tracing) | 0/10 | 1.233 | 0.500 |
| QML, normal logging | 100/100 | 1.033 | 0.500 |
| Final Bead CPU renderer, normal logging | 100/100 | 0.400 | 0.533 |

The first focus-only fix used 1.033% core and failed the 1% gate; the bound-width
cache closes that gap. Final inputs share identical palette and wallpaper hashes.
Full CI passes. [Source-pinned evidence and limits](docs/specs/0119-click-focus.md).

## 2026-10-25 — B0106 dashboard review candidate

Asynchronous poster preparation removes the long first-content wait.
Five alternating CPU pairs pass closed recovery; open growth is not fully isolated to image bytes.
Three cold image-distance pairs meet the median target, with one late outlier; warm/close still differ.
Media, Weather and System look closer; Today is unchanged in whole-output RMSE.
Full remote Arch CI passes. No allocator environment or frame-pacing change ships.

| Matched private CPU measurement | Before | Selected |
| --- | ---: | ---: |
| First content commit / presentation, ms | 271.0 / 335.1 | 57.2 / 101.8 |
| Cold image-distance t10 median, ms (QML 119.8) | 154.4 | 113.6 |
| Closed paired Δ Today / Media, MiB | historical B0102 +0.36 / +1.05 | +0.160 / −0.039 |
| Closed paired Δ Weather / System, MiB | historical B0102 +0.95 / +1.20 | +0.059 / −0.125 |
| RGB RMSE Today / Media, 0–255 | 15.143 / 15.513 | 15.152 / 13.927 |
| RGB RMSE Weather / System, 0–255 | 17.318 / 19.822 | 14.974 / 16.183 |

Historical B0102 deltas are not a new matched series; selected deltas use the original control in five pairs.
Open image-only acceptance and consistent motion remain unresolved; no laptop-cap claim.
[Stages, per-page components, rejected experiments and all 13 visual states](docs/specs/0106-dashboard-followup.md).

## 2026-10-25 — B0105 rejected callback replacement

An extra pending-frame replacement increases matched updates but leaves CPU latency above target.
QML is slower in this private chain, so the live comparison is not reproduced.
A private-parent refresh diagnostic shows the nominal niri mode is insufficient to establish cadence.
One +1.516 MiB anonymous closed-memory outlier requires a stop; the GL candidate was canceled.
No engine scheduling change is integrated. Full probe-only remote Arch CI passes.

| Measurement | Control | Rejected experiment |
| --- | ---: | ---: |
| CPU hover p50/p99 | 32.09/32.13 ms | 32.09/32.12 ms |
| CPU drag p50/p99 | 66.45/77.15 ms | 66.19/71.62 ms |
| QML hover / drag p50 | reference | 95.72 / 98.13 ms |
| Home first-key CPU median | 7.313 ms | 7.323 ms |
| Home idle/open/closed paired unreclaimable Δ | reference | −0.035/−0.156/+0.090 MiB |
| Same control, parent 60→120 Hz, hover p50 | 32.09 ms | 16.08 ms (diagnostic) |

[Stages, gates, memory outlier, sources and paired images](docs/specs/0105-present-latency.md).

## 2026-10-25 — B0104 pearl drag/close, lead-accepted memory exception

Logical dragging and native-window close repairs pass the full remote Arch checks.
Three alternating private CPU pairs fail detached/closed unreclaimable memory.
The +2.133 MiB detached anon outlier originally tripped the owner stop line; its cause is unresolved.
**lead-accepted 2026-10-25, follow-up memory attribution** prioritizes the unclosable-pearl fix; final parity proofs remain incomplete.

| CPU debug, paired median Δ | Unreclaimable MiB | PSS MiB (info) | ≤+0.5 gate |
| --- | ---: | ---: | --- |
| Idle | +0.008 | +0.197 | pass |
| Docked | −0.367 | −0.260 | pass |
| Detached | +0.891 | +1.255 | **fail** |
| Closed | +0.828 | +0.992 | **fail** |

DRM and SwapPss are verified zero; file-backed deltas are informational.
A current-base diagnostic pair gives detached/closed +0.531/+0.320 MiB, with
+295/+412 KiB allocator in-use and backing-granularity variation; this is not a new median.
No further product change was justified by that pair.
[Components, attribution, close checks and remaining proofs](docs/specs/0104-pearls-drag.md).

## 2026-10-25 — B0097 joined panel reveals and status anchors

Launcher/control frame joins and invoking-icon status placement are implemented.
Private matched CPU captures show much closer popouts and hover cards.
The first closed-memory cohort failed at +1.496 MiB; unused shadow asset copies were removed.
The changed three-pair cohort passes; allocator variance and launcher timing gaps remain.

| Measurement | Before | After |
| --- | --- | --- |
| First-key CPU median, three fixture pairs | 5.733 ms | 5.750 ms |
| Full-resolution closed UI unreclaimable | 25.076 MiB | 24.986 MiB |
| Six popout ROI RMSE, median | 29.663% | 6.575% |
| Six hover ROI RMSE, median | 22.550% | 6.482% |
| Right-lip ROI RMSE | 37.828% | 7.859% |

Relative memory gates and full remote CI pass; absolute laptop memory is unmeasured.
[Source conditions, component breakdown, failure and residual parity gaps](docs/specs/0097-panel-anchors-owner.md).

## 2026-10-25 — B0103 integrated control feedback

Sliders preview locally; queued motion coalesces and host writes keep their latest target.
Remote full CI and private CPU/native-GL captures pass; drag presentation improves substantially.
All four absolute 24.67 ms hover/drag gates still fail in the nested chain; physical laptop timing is unmeasured.
Six Home pairs pass the 8 ms first-key and +0.5 MiB median memory gates; the +2.496 MiB closed outlier is retained.
Allocator follow-up shows stable live heap/cache sizes with varying anonymous mapping residency.

| Private measurement | Control | Final |
| --- | ---: | ---: |
| CPU hover p50/p99, ms | 32.20/32.45 | 32.29/32.48 |
| CPU drag p50/p99, ms | 782.63/1225.09 | 66.30/70.46 |
| GL hover p50/p99, ms | 32.12/32.34 | 32.30/32.50 |
| GL drag p50/p99, ms | 935.15/1454.08 | 66.26/78.37 |
| Home first-key median, ms | 6.802 | 7.101 |
| Home idle/open/closed paired Δ, MiB | reference | +0.037/+0.203/−0.014 |

[Stages, memory decomposition, source hashes and appearance gaps](docs/specs/0103-control-latency.md).

## 2026-10-25 — B0100 wallpaper picker

Super+W opens a real Luau picker backed by the additive Nacre catalogue contract.
Thirteen guarded private QML/Bead states pass; layout and buffer fixes are included.
Five final matched pairs fail the relative closed-memory gate; the excess is anonymous.
Full remote Arch CI passes; this remains a review candidate with explicit visual gaps.

| CPU, 2880×1800 at scale 1.5 | Result |
| --- | ---: |
| Carousel / spotlight / hexagon surface RMSE | 6.055 / 4.879 / 3.479% |
| Dynamic spotlight / video-activation surface RMSE | 8.745 / 7.269% |
| Carousel open / close t10–t90 proxy, QML → Bead | 186 → 290 / 116 → 166 ms |
| Cold fullscreen first image, spotlight / hexagons | 321 / 292 ms |
| Earlier closed spotlight / hexagon paired median Δ | +1.871 / +2.023 MiB (fail) |
| Final five-pair idle / carousel / spotlight / hexagon closed Δ | +0.102 / +0.617 / +0.586 / +0.633 MiB (closed fail) |

[Source conditions, paired images, memory breakdowns and known gaps](docs/specs/0100-wallpaper-picker.md).

## 2026-10-24 — B0080 populated Home first key

Home still opens first; a ready search reply replaces superseded work before raster.
Bounded generation-owned markup fragments reuse Sidebar, Quick and prefix hints.
Partial buffer adoption and on-use page advice reduce cold repair work.
Five matched private CPU pairs and full Arch CI pass; real-engine typing remains unmeasured.

| Five-pair medians, CPU backend | Main | Candidate / paired Δ | Gate |
| --- | ---: | ---: | --- |
| Home first key, ms | 15.161 | 7.598 | ≤8 pass |
| Standalone first key, ms | 9.437 | 6.508 | ≤8 pass |
| Later Home / standalone, ms | 13.682 / 7.610 | 6.811 / 7.031 | ≤8 pass |
| Opening paired Δ, ms | — | −0.683 | ≤+2 pass |
| Idle / open / closed total Δ, MiB | — | −0.165 / +0.039 / −0.316 | ≤+0.5 pass |

[Stages, residency, pixel pairs and limits](docs/specs/0080-home-first-key.md).

## 2026-10-24 — B0075 first-key repair

An adjacent-commit bisect identifies cold opening motion, not Home routing.
The final pose now renders on time; stale SHM slots can borrow fresher pixels.
Shared immutable rasters and bounded repairs cut work without changing the UI.
Specified window median gates and full Arch CI pass; Home-bar typing remains a separate gap.

| CPU, five private pairs | Main | Candidate / paired Δ | Gate |
| --- | ---: | ---: | --- |
| First key, ms | 17.953 | 7.259 | ≤8 pass |
| Later a / BackSpace, ms | 6.631 / 7.878 | 6.379 / 7.505 | ≤8 pass |
| Opening paired Δ, ms | — | +0.066 | ≤+2 pass |
| Idle / closed total Δ, MiB | — | +0.054 / −0.067 / −0.278 | ≤+0.5 pass |

[Method, memory splits and limits](docs/specs/0075-first-key-regression.md).

## 2026-10-24 — B0074 dashboard admission and reveal preparation

Starting the reveal clock at admission removes worker/configure time from its start.
Content-sized contour SVGs avoid rasterizing transparent rows on early frames.
Three nested CPU pairs meet the median 10% ±20 ms goal; full remote Arch CI passes.
The inherited first-key CPU gate still fails in both builds; see B0074 for spreads and limits.

| Metric | Control | Candidate | QML / gate |
| --- | ---: | ---: | ---: |
| Cold 10% image progress, ms | 256 | 197 | 201 |
| Warm 10% image progress, ms | 250 | 205 | 200 |
| Closed UI PSS+DRM+SwapPss paired Δ, MiB | — | +0.262 | ≤+0.5 |
| First-key UI CPU, ms | 17.991 | 17.915 | ≤8 fails both |

## 2026-10-24 — B0072 launcher Quick controls

The launcher gains scoped controls and a published updates-count consumer.
Fake-daemon and native pointer evidence pass; unknown state stays disabled or hidden.
Memory and opening gates pass. First-key CPU still misses 8 ms in both builds.
Full remote CI passes; this is a review candidate, with no live switch.

| CPU renderer, five private desktop pairs | Main | Quick |
| --- | ---: | ---: |
| First key / `a` / BackSpace CPU ms | 17.656 / 6.720 / 9.005 | 18.359 / 6.670 / 7.952 |
| Launcher open ms | 29.978 | 30.001 |
| Final closed PSS+DRM+SwapPss MiB | 11.195 | 11.170 |

Paired median open delta −0.116 ms; closed delta +0.044 MiB. DRM/SwapPss zero.
See [B0072](docs/specs/0072-launcher-quick-controls.md) for category splits,
rejected mask shortcut, guard retry and remaining contract/visual gaps.

## 2026-10-24 — B0071 growing frame and status popout parity

The dashboard now redraws its joined contour at each reveal height.
Status popouts attach to the bar instead of the control dock, with matching body colour.
Private nested CPU captures show lower errors; visual/control gaps remain in B0071.
Full remote CI passed. Relative UI memory was not measured in this unit.

| Whole-image RMSE % | Before | After |
| --- | ---: | ---: |
| Wi-Fi | 17.825 | 10.248 |
| Bluetooth | 15.277 | 5.532 |
| Battery | 14.339 | 4.846 |
| Audio, matched initial width | 19.316 | 7.200 |

See [B0071](docs/specs/0071-visual-parity-5.md) for normalized audio setup,
per-progress evidence, pointer/OSD fixtures and limits. No live validation.

## 2026-10-22 — B0025 per-output sessions: acceptance passes

Outputs own runtime/view/update/commit/input lifetimes; services and immutable resources remain shared.
Scale changes retire old caches and pools; idle release returns extra painted frames and allocator slack.
Matched CPU release captures pass the single-output +0.5 MiB gate and the extra-output ~3 MiB target.
The original desktop absolute-cap stop and two-output +0.5 MiB verdict are superseded by owner rulings.

| CPU UI PSS metric, MiB | Before | Final |
| --- | ---: | ---: |
| niri initial 1.5 vs control | +0.443359 | -1.401367 |
| niri changed 1.25 vs control | +0.883789 | -0.879883 |
| niri return 1.5 vs control | +0.981445 | -1.016602 |
| Two-output anonymous vs control | +1.433594 | +0.179688 |
| Extra-output candidate cost | +2.044922 | +1.862305 |
| Hyprland one output vs control | +0.269531 | -0.992188 |

Before/final each use their own matched main control; these are acceptance deltas, not a direct speedup study.
Captured source/control `515ecf7`/`b972a31`; integrated `8b1f7ba` has identical product/proof files.
Rust 1.99.0, default release features, CPU renderer.
Five alternating pairs/compositor, 10 s warmup + five 1 s samples/phase; geometry matches.
Median extra-output UI PSS is **+1.862305 MiB** (range 1.818359–1.885742), passing the
~3 MiB target. Anonymous +1.191406, file -0.000977, SHM +0.671875 MiB. Categories are
independent medians and need not sum exactly.
SwapPss/host swap-in zero; DRM unavailable. PSS categories do not sum with heap counters.
First-pair added malloc-live 1.033 MiB: named charges 0.678 (Luau 0.465), unclassified ~0.356 MiB.
Free arena space +0.475 MiB is separately measured; temporary markup VM/layout arena are not retained.
Full Arch Bead CI, private input/idle/scale/remove/re-add proof and niri scale arms pass.
Niri Winit exposes one output and no supported private create/remove API; multi-output hotplug is unproved there.
No live change. See [B0025](docs/specs/0025-per-output-ui-sessions.md) for hashes, ranges, counters and commands.

## 2026-10-22 — B0021 wallpaper memory diagnosis

The reported 26.64 → 59.79 MiB jump compared CPU-only release with default native GL release.
Matched CPU builds show that buffer release works; decoder/library residency was a separate real cost.
A short-lived companion decoder removes GdkPixbuf/Glycin and their threads from the settled UI.
The fixed wallpaper overhead is +0.251 MiB on Hyprland and +0.619 MiB on niri, excluding niri's compositor-held wallpaper buffer.

| CPU release build | Hyprland PSS; file / anon / SHM (MiB) | niri PSS; file / anon / SHM (MiB) |
| --- | ---: | ---: |
| Main `a6d4233` | 9.089; 7.532 / 1.293 / 0.264 | 9.019; 7.474 / 1.281 / 0.264 |
| Pre-release cleanup `c919e34` | 25.509; 12.860 / 2.496 / 10.152 | 26.206; 13.573 / 2.480 / 10.152 |
| Post-release cleanup `a3aaca1` | 15.295; 12.680 / 2.352 / 0.264 | 25.868; 13.228 / 2.488 / 10.152 |
| Worker decoder `ac27ca3` | 9.340; 7.725 / 1.352 / 0.264 | 19.526; 7.925 / 1.449 / 10.152 |

All builds used `cargo build --release -p nacre-shell --no-default-features --locked -j2`.
Same 1×1 PNG fixture, 2880×1800 at scale 1.5; Hyprland virtual 120 Hz, niri nested 60 Hz (not physical scanout timing).
Harness flags: `spikes/measure.py --compositor <hyprland|niri> --parent headless --runs 1 --seconds 1 --modes bead --lifecycle`, plus `--virtual120` on Hyprland; five seconds after first callback, then one second idle. niri's viewport resize proof replaces its obsolete integer-scale assertion.
An instrumented `measure-memory.py` copy saved full smaps at the existing before/after samples; the transition copy additionally drained stdout and sampled every 25 ms.
Private HOME/bus/runtime, desktop lock, containment/watchdog and swap gates passed. SwapPss and host swap-in were zero; no DRM fdinfo was present. Executable hashes, rollups and full per-mapping smaps were retained privately.

Native GL main/post measured 54.898/58.873 MiB: file 26.490/31.825, anonymous 28.320/26.961. NVIDIA compiler, LLVM and Gallium mappings plus heap account for the renderer difference; it is not a new decode Vec or a debug build.
CPU decoder residency included 3.271 MiB Glycin PSS, 0.460 MiB GIO and 0.264 MiB GdkPixbuf. Settled threads fell from nine to three. The fixed UI no longer links those libraries.
Heaptrack: old decoder peak live heap 75.20 MB; live heap during seconds 5–14 was constant at 535,139 bytes, with 315.72 KB remaining at exit. A longer uninstrumented post-cleanup arm settled at 15.457 MiB, ruling out a short settle window.
Hyprland releases wallpaper SHM. niri logs `WALLPAPER_BUFFER_RETAINED ... reason=no_release`; its additional 9.889 MiB wallpaper SHM PSS remains intact, as required by the compositor fallback contract.
One revision-1 → revision-2 crossfade passed with the small surface and private animation-policy hook: UI PSS peaked at 78.653 MiB and returned to 10.069 MiB (7.892 file, 1.387 anon, 0.791 SHM); the decoder worker is transient and is reaped before commit. Peak is UI-only, not combined worker/compositor memory.
Full Bead CI passed in the prod Arch container, including worker pipe/identity/bounds tests. CI provisioning needed DejaVu fonts and a git snapshot for existing font/provenance tests; no desktop packages changed.
Ship `nacre-wallpaper-decode` beside `nacre-shell`; package builds produce both. No new codec dependency, install, live run or push. Native GL's pre-existing absolute memory remains outside acceptance; this validates the original CPU comparison.

## 2026-10-22 — B0019 launcher parity diagnostic

Async queries retain same-scope rows, fence stale routes and preserve focused input.
Native pin/unpin, navigation and idle-reopen proofs passed on both private compositors.
Serial query/item identity and RPC RTT now reach exact commit/presentation joins.
First-key CPU remains a gap; these candidate-only diagnostics do not accept Gate E.

| Proof or measurement | Hyprland | niri |
| --- | ---: | ---: |
| 200-key diagnostic | Partial; post-check failed | Valid |
| Steady presented p50 / p99 (ms) | 9.380 / 9.871 | 5.857 / 6.269 |
| First-key presented / UI CPU (ms) | 25.214 / 13.905 | 25.016 / 14.777 |
| Steady UI CPU / RPC RTT p99 (ms) | 5.119 / 2.581 | 5.351 / 2.728 |
| Maximum closed-minus-bar PSS (MiB) | +0.989 | +0.079 |
| UI SwapPss / DRM | 0 / unknown | 0 / unknown |

Steady excludes first key. Hyprland's raw arm remains invalid after a missing
UI_STATE waiter; the fixed two-key contract smoke passed. Niri passed all 200
keys, refresh, pin/unpin readback and safe group activation. Private outputs
were 2880×1800 / scale 1.5, synthetic 120 Hz / 60 Hz; no physical or matched QML
verdict. Closed anonymous growth was investigated before further captures:
trace live heap +0.262 MiB versus free arena +18.210 MiB supports allocator
residency. End-of-typing niri UI PSS was 47.161 MiB; daemon 34.812 and worker
21.066 MiB remain separate. No matched or absolute memory clearance.
Full Arch CI passed `c8b55b5`; no renderer change, new desktop packages, push or
live run. [B0019](docs/specs/0019-launcher-parity.md) records provenance and limits.

## 2026-10-22 — B0018 status-pill interaction

One pill now routes the four existing device views through hover peeks and clicks.
Peeks remain nonmodal; clicks open compact panels at the usable top-right.
Private native input and full CI establish behavior; physical timing and memory remain unmeasured.
The existing QML placement/status goldens remain unchanged and pass.

| Configuration or proof | Result |
| --- | --- |
| Icon center spacing / clicked body | 26 px / 400×286 px |
| Configured dwell / peek leave | 350 / 120 ms |
| Initial crossing / entered-panel leave | 600 / 130 ms |
| Peek / clicked reveal | 230 ms InOutCubic / 280 ms OutCubic |
| Machine / native input-bound unit tests | 17 / 3 passed |
| Observed smoke hover / pointer-away close | 0.401–0.476 s / 0.164–0.179 s |
| UI PSS / DRM / SwapPss | Not measured / unknown / not measured |

Source `c6ba4b3` passed the full remote Arch workflow and exact-source nested
Labwc CPU smoke. Observed times include IPC/polling; they are not matched QML
or physical scanout measurements. Native proof found a warmed preview without
a surface; input-bound review also corrected shadow padding over the bar.
An existing watchdog deadline flaked once; unchanged isolated/full reruns
passed, cause unconfirmed.
Test prerequisite addition: `ttf-dejavu` inside the CI container only.
The baseline has no edge-charge graphic. No live switch, install or push occurred.

## 2026-10-22 — B0016 tray preview boundary

The existing headless tray host now supports the bar preview and lazy menus.
Private protocol and nested CPU checks establish behavior, not memory clearance.
Core-mode enablement still needs published daemon ownership and action contracts.
No current QML tray was found, so no QML tray parity verdict is available.

| Limit or measurement | Value |
| --- | --- |
| Retained item identities | 64 maximum |
| Menu entries / nesting | 256 / 8 maximum |
| Retained item icon bytes | 1 MiB maximum per item |
| Menu viewport | At most 400 logical pixels high |
| UI PSS / DRM / SwapPss | Not measured / unknown / not measured |

The 25 MiB ceiling, 24.5 MiB stop line and own-bar+5 MiB close rule remain open
for matched captures. Source bounds are not a measured memory result. zbus
materializes wire messages before these retention caps. Additional validation
packages: dbus, labwc, mesa, ttf-dejavu, noto-fonts and noto-fonts-emoji, installed
only in the private CI container. No new Cargo dependencies or live changes.
Nested opening exposed a repeated configure reattaching a busy dismissal buffer;
the immutable pixel now attaches once. Native menu retirement is epoch-checked.


## 2026-10-20 — B0002 Hyprland combined-legacy stop

A1's four settled endpoints passed after strict validation of existing raw JSON. B1 passed three endpoints, then the required combined-legacy settled total reached 24.541992 MiB, above the 24.5 MiB stop line. The prior control was 24.010742 MiB. This single pair is a stop signal, not causal attribution or a regression verdict; no threshold change or retry is authorized.

| Settled UI PSS+DRM+SwapPss MiB | Never | Once | Several | Combined legacy |
| --- | ---: | ---: | ---: | ---: |
| A1 control | 23.698242 | 23.705078 | 23.702148 | 24.010742 |
| B1 candidate | 23.826172 | 23.783203 | 24.035156 | 24.541992 — STOP |

A1 and B1 used the frozen UI/helper/fixtures and identical Hyprland geometry and phase windows. B1's per-close transient 24.606445 MiB was below the 25 MiB close rail and is not the settled endpoint. Raw evidence remains private; details and SHA-256 values are in the private capture report.

## 2026-10-17 — Published compatibility protocol passes consumer gates

Adopt full-green Nacre5f4c0bf3; reserved schemas do not advertise runtime hosting.
Seven unchanged upstream examples replay through the real store Mirror.
Production view sources and concrete per-page decoding are unchanged.
Nested lifecycle gates fit the existing ceiling; no speed or optimization claim.

| Check / UI MiB H /N | Result |
| --- | ---: |
| Rust workspace / Luau modules |236 /52 pass|
| Typed positive / negative / semantic |100 /7 /32 pass|
| Original fixtures / new snapshots+patches |30 exact /7+29|
| Final settled / max closed |23.756/24.258 /23.760/24.289|
| Settled file / anonymous |15.859/15.973 /7.203/7.180|
| Settled shared / SwapPss |0.693/1.105 /0/0|
| Close/settled samples / process audit |76 pass /0|

Settled endpoints <24.5, all close/lifecycle <=ownbar+5 and <=25MiB.
Relative to prior separate native runs H+0.070MiB is file+0.074/anon−0.004;
N−0.197 is file−0.018/anon−0.180, shared unchanged. Not paired attribution.
Release24,548,576B (+38,728; text+15,104/rodata+192/data+bss unchanged).
Decoration SHM542,592B unchanged; tracked caches197,559/195,440B after idle.
Single nested run/compositor, same frozen75157 helper/4pages/20cycles and final
idle; no own build/test overlap, shared host. DRM/refresh unknown, not physical.
Fmt, all-feature Clippy-Dwarnings, trace, CPU-only/release and service boundary
pass. Seven expected Luau rejections include the corrected popup_order ID type;
old failing proof is preserved. All eight page PNGs remain unchanged.
No new runtime backend, action executor, UI control or helper is claimed.
CI39056831629 passes all but new unmaintained advisories0370/0371/0372 for
unchanged zbus5 dependencies. No policy waiver; latest AccessKit/AT-SPI/tray
still usezbus5. STOP for owner exception vs broader migration decision.
Typing targets, Reader application trimming and optional-page scope stay open.

## 2026-10-17 — Four core dashboard tabs fit the unchanged ceiling

Today adds a lazy calendar/clock/agenda using concrete published page types.
All page resources release after closed+idle; kernel residency is measured too.
Five alternating pairs and both full native proofs pass; keep the25MiB ceiling.
The smaller typing brush-hoist screen is rejected separately, with no code kept.

| UI MiB; H /N | Before → candidate medians |
| --- | ---: |
| Never opened |21.800 → 21.826 /22.968 → 22.957|
| Once opened/closed |21.930 → 22.509 /22.604 → 23.080|
| Repeated opens/closes |22.098 → 22.742 /22.672 → 23.264|
| Repeated paired delta |+0.644 /+0.592|
| Full native20 final settled |23.686 /24.455|

All376 close/settled observations pass ownbar+5/25 and settled<24.5; SwapPss0.
Post-use paired file+0.339/+0.367, anonymous+0.223/+0.156MiB, shared unchanged;
component medians do not add. Clean executable pages explain most file growth;
exact smaller anonymous attribution remains unproved, below1MiB. Binary+37,472B
(text+30,080B). Root5sources/Luau502,024B, caches194,403/195,628B, frame SHM542,592B.
234 tests/fmt/Clippy/CPU-only/release/Luau52 pass; two Today PNGs, prior six exact.
A real undeclared-field failure is fixed and preserved; no proof relaxation.
Nested/shared host, no own build/test/capture overlap; DRM/refresh unknown, no
physical120Hz claim. Real topics/updated schemas unadvertised/unpublished; c2
pin stays. Optional extras/QML comparison/providers remain. [Report](docs/research/feature9-gate-d.md).

## 2026-10-17 — Launcher brush hoist rejected

A small Luau-only typing probe reused two immutable tint records per module.
It retained all row/property validation and matched three accepted PNGs exactly.
The first matched screen has mixed CPU direction, so keep production unchanged.

| One 200-key pair per compositor | Hyprland before → probe | niri before → probe |
| --- | ---: | ---: |
| UI CPU mean, ms/key |9.306 → 9.191|10.478 → 10.625|
| Presented p99, ms |25.070 → 30.490|34.447 → 30.580|

All 800 exact key presentations and owned cleanup pass; same executable and
frozen 75157 helper, 320 apps / 64-row replies. One pair is neither a spread
estimate nor evidence of a tail regression. Reject before longer acceptance
trials; no memory gain or shipped optimization. UI 8 ms / presented 16.7 ms
remain unmet. Nested H virtual 120 / N 60, scale 1.5; feedback refresh unknown,
shared-host contention uncontrolled, no own builds/tests/capture overlap.
Raw ignored dashboard-brush-screen / dashboard-brush-pixels; [report](docs/research/feature9-gate-d.md).

## 2026-10-17 — Dashboard code cost resolved within the existing ceiling

Lazy page payloads already release after closed+idle; the broad protocol decoder
also linked parsers for unrelated services into the view. Decode only the four
dashboard payload types and keep the same validation. Five alternating pairs
pass every warm/closed gate; the original lifecycle now fits the24.5 MiB line.

|Measure|Hyprland / niri|
|---|---|
|Never-opened medians, before → after MiB|21.689 → 21.796 /23.098 → 23.113; inside control spread|
|Once per page medians|22.198 → 21.942 /23.020 → 22.640|
|Three times per page medians|22.376 → 22.142 /23.179 → 22.815|
|Median paired post-use change MiB|-0.443 /-0.325; marginal medians differ|
|Paired file /anon /shared MiB|-0.489/-0.027/0 ;-0.345/-0.008/0|
|Original full 20-cycle final MiB|23.603 /24.158; below 24.5, keep 25 ceiling|
|Full20 closed max /ownbar delta MiB|23.657/+2.124 ;24.139/+1.125|
|Guards|240 focused+66 full close/settled samples pass; SwapPss 0|
|Release file /text|−666,848 B /−518,272 B; data/bss unchanged|
|Checks|230 workspace, trace, fmt/Clippy, CPU-only/release, Luau (47 modules); six unchanged PNGs|

Matched35/45/60 s phases leave>=30 s idle, same320 apps/frozen75157 and no own build
or test overlap; shared local nested host, not physical/quiet. CPU DRM is null,
refresh 0 unknown. Five-pair comparison decides the narrow decoder; full 20-cycle is a
confirmation, not another five-pair comparison. No typing/QML saving claim.
Core c2 schema stays latestgreen; revised OSD/core additions await publication+CI.
First pages stop for review. No dependency or live installation; process audit0.
[Method and caveats](docs/research/feature9-gate-d.md#first-dashboard-pages--memory-resolved-2026-10-17).

## 2026-10-17 — First dashboard pages reach the headroom stop

Three lazy dashboard pages compile and render against reserved schema fixtures.
Their local tests and private headless preview pass; runtime remains unpublished.
All close budgets pass, but Niri settles inside the owner's new memory reserve.
The batch stops before paired controls or typing; no gain or parity is claimed.

| Nested result, H / N | Result |
| --- | --- |
| Local checks |230 workspace + trace/fmt/Clippy/CPU/release/Luau47 pass|
| Populated page goldens |Weather/Media/System at scales1/1.5; repeat pass|
| Unavailable pages, final settled MiB |23.822 /24.413|
| Fixture native20, final settled MiB |24.423 /24.768; N trips24.5 stop|
| File / anonymous / shared MiB |16.483/7.246/.693 /16.510/7.152/1.105|
| Closed / final observations |106 /4, all ownbar+5; all captures hard-guard valid|
| Maximum closed delta from own bar MiB |2.490 /1.572|
| Fixture page warm UI medians MiB |45.580–45.852 /45.280–45.970, two samples/page|
| Registry cache bytes at report |195885 /194114; permanent frame pool542592 raw bytes|
| New headroom gate |FAIL, no COMPLETE; propose26 ceiling/25.5 stop|

Methods: four serial private nested captures, frozen75157 daemon, same immutable
release; light metadata only, no own builds/tests. DRM counters null, not a
measured zero; configured CPU. SwapPss0. Refresh interval0 means unknown.
No matched QML or old/new paired measurement completed. See research report.

## 2026-10-17 — Placement and joined frame accepted

The owner accepts the exact measured d88e94e placement candidate.
Its source is unchanged; only integration history and current documentation differ.
The per-feature allowance is replaced, while close and absolute budgets remain.
Dashboard pages continue next, with a stop before24.5MiB settled UI.

| Nested/local evidence, H / N | Result |
| --- | --- |
| Five pairs plus native20 |424 close/settled observations pass ownbar+5MiB|
| Maximum final settled UI MiB |23.096 /23.648|
| Paired settled increase MiB |+0.729 /+0.861, explicitly accepted|
| File / frame-shared / anonymous medians MiB |+.313/+.260/+.098 /+.428/+.260/+.109|
| Source identity |integration code equals d88e94e; prior220 tests+trace/fmt/clippy/CPU/release pass|

Historical allowance failure below remains evidence, not the current owner gate.
No physical measurement, dashboard result or typing improvement is claimed.

## 2026-10-17 — Attached placement and joined-frame candidate

The joined frame matches canonical Nacre placement, including flush popouts.
Its real memory cost exceeds the per-feature allowance, so it is not integrated.
The absolute ceiling and all close budgets pass; the separate increase gate fails.
Dashboard drafts are preserved unbuilt while the owner decides this frame cost.

| Nested/local result, H / N | Result |
| --- | --- |
| Real-Qt geometry/rim + paired placement masks |11 pass at2880×1800/1.5; exact solid interiors|
| Candidate local checks |220 +trace/fmt/clippy/CPU-only/release pass|
| Strict mapped-close regression |4ace failed; fixed d88 native20 passes both|
| Five-pair settled UI medians MiB |22.292→22.946 /22.652→23.546|
| Median paired delta / control spread MiB |+0.729/0.216 /+0.861/0.440|
| Maximum settled candidate UI MiB |23.096 /23.648, both≤25|
| Close/settled samples, all / candidate |424 /244, all ownbar+5; max candidate delta2.119/1.048|
| First-open median paired delta MiB |+0.418 /+0.592|
| Warm-third median paired delta MiB |+0.327 /+0.730; N ranges span~6.8MiB|
| Settled file / shared / anonymous paired medians MiB |+.313/+.260/+.098 /+.428/+.260/+.109|
| UI + daemon/workers max settled MiB |55.121 /55.383; no combined notices/wallpaper|
| Feature allowance / outcome |0.5MiB; FAIL both; no COMPLETE or preview update|

Five alternating pairs each; same frozen117 control/helper75157, normal policy,
shared host with light editing/metadata but no concurrent own builds/tests/captures.
These are nested resource results, not physical timing or typing improvements.
Medians of component deltas do not sum to the median total delta. Eight static
tiles hold542592pixel bytes; no full-output raster/GL/decoder cache is retained.
No oversized eager pool was found; dropping canonical rim geometry is not a fix.
Source d88 is unpublished; dashboard911688e is unbuilt/unvalidated. The owner
requests a top launcher, while the published QML host selects bottom; both
canonical attached() cases pass. Masks are placement, not full content parity.
[Full scope and evidence](docs/research/feature9-gate-d.md#dashboard-c-authorized-placement-candidate--2026-10-17).

## 2026-10-17 — Lazy read-only service popouts

Network, Bluetooth and battery share the existing hosted service through Sources.
Factories keep small configuration; modules/data load on open or hover.
Closed workers retire after 2.2 seconds; unused hover expires after 2 seconds.
Device actions/backends and daemon hosting are not invented to fill contract gaps.

| Nested measurement, H / N | Result |
| --- | --- |
| Five-pair settled UI medians, control → candidate MiB |23.205→23.354 /23.756→23.952|
| Paired settled delta / control spread MiB |+0.069/0.294 /+0.090/0.188|
| Max settled UI, including native20 MiB |23.831 /24.482, both ≤25|
| Close/cap/group/cleanup checks |394 samples,244 candidate, all pass bar+5; controls valid|
| Lazy compiled/view owners |48 status pairs,2→1 on every close; only bar remains|
| 60s Bead open-status UI MiB |26.088–27.568 /26.980–27.052|
| Matched QML unavailable status/header MiB |184.521–190.371 /176.366–180.194|
| One typing pair, UI ms/key /presented p99 ms |9.963/30.886 /10.922/31.562; targets not accepted|
| Checks |213 workspace + trace; fmt/clippy-Dwarnings/CPU-only/release pass|

All totals are PSS+DRM+SwapPss. Pair deltas are medians of matched differences,
not differences of overall medians. No settled/peak saving or extra exception.
Closed cache reports162,367–329,658B fit caps; transient bar scratch explains
large reports. Payload/source weak-reference and unrelated4MiB tray tests pass.
Six populated scale1/1.5 goldens; unchanged Source wakes stage no scene.

QML copied visual dependencies54a9 unchanged through e0a81ce7. Same popup text,
geometry/tokens/palette, unavailable state; header fixtures/shadows/service activity
vary. No full service/hardware/CPU parity claim. Both native real bar-click references
pass36 further closes. Both updated headless private previews stop all owned clients.
Typing uses standalone CPU launcher/backendnone,320apps,frozen75157;800 exact
key/result/presentation proofs pass. One pair proves no typing improvement;
UI>8 prevents the two-frame candidate. Shared QML/executor/full services pending.

2880×1800/scale1.5,Hvirtual120/Nnested60,refreshunknown;shared host/light work
concurrent,no own build/test/capture overlap;not physical. B is incomplete pending
service contracts/hosting; stop review before dashboard. QML remains daily driver.
Raw core-status-views-117d66b-{memory-five,reference,typing-screen,preview,smoke}.
[Full scope/forecast](docs/research/feature9-gate-d.md#lazy-read-only-service-views--2026-10-17). [CI 39003597141 green](https://github.com/nacre-desktop/bead/actions/runs/39003597141).

## 2026-10-17 — Replay the green typed UI-action contract

Pin Nacre's green protocol revision and its supplementary action transcript.
An injected provider queues a typed action and ACKs before the held reply.
The real shell keeps only its existing dispatch capability advertised.
No executor, backend or launcher provider is added.

| Check | Result |
| --- | --- |
| Supplementary transcript / internal provider calls |19 frames /5|
| Workspace tests / trace / fmt / clippy / CPU-only |206 /pass /pass /pass /pass|
| One-pair settled UI MiB, H /N |22.900→23.161 /23.618→23.808|
| Strict closed/settled observations |44 passed|

Typed bounds/roundtrip and bounded queue ACK ordering pass against a fake daemon.
Normal immutable binaries, same75157 engine; all close+5/25/cap/group/cleanup guards
pass. Settled +0.261/+0.189 fits prior spreads0.442/0.689; no extra allowance.
One native pair/compositor is compatibility coverage, not new typing acceptance.
Nested scale1.5 Hvirtual120/N60, unknown refresh/shared-host contention, no own
build/test/capture overlap. Executor and same-engine QML remain upstream.
[Contract and measurement scope](docs/research/feature9-gate-d.md#typed-ui-action-contract-replay--2026-10-17).

## 2026-10-17 — Keep ranked prefixes; typing target remains open

Edited queries request a bounded prefix from Nacre's existing engine.
Scrolling and selection expand before reaching unloaded results.
Five alternating pairs show a modest UI CPU win on both compositors.
Warm/settled memory stays within control spreads; no renderer change.

| Five-pair medians, Hyprland / niri | Control → prefix |
| --- | --- |
| UI CPU, ms/key |10.172→9.793 /11.025→10.560|
| Presented p99, ms |30.198→30.914 /32.781→32.953|
| Third warm UI MiB |48.867→48.917 /47.073→47.083|
| Settled UI MiB |23.219→23.039 /23.852→23.711|

4,000 exact key→result→commit→presentation proofs, 320 apps, frozen75157 engine.
Ranked replies contain101 rows full/64 limited; initial/catalog wakes stay full.
UI paired −0.583/−0.575 ms; tails fit spreads8.155/7.588 ms. UI8 ms/key and
p9916.7 ms remain unaccepted; UI>8 prevents the two-frame candidate rule.
Both native20 proofs pass; all168 candidate close/settled observations fitbar+5,
max settled23.251/23.988<25. Caps/group/cleanup pass; all controls valid.
Warm anonymous-page ranges vary widely; paired-spread pass is not a peak reduction
or settled saving. 204 tests, trace proof, fmt/clippy/CPU-only pass. Nested CPU
scale1.5, Hvirtual120/N60, refresh unknown; shared host/light editing concurrent,
no own build/test/capture overlap. No physical or full-service parity claim.
[Ranges, separate daemon/provider CPU and method](docs/research/feature9-gate-d.md#ranked-prefixes-for-edited-queries--2026-10-17).

## 2026-10-17 — Calendar Source wakes and transient route restoration

Calendar follows the clock until browsed, including Source-only wakes.
Reload preserves that choice; retired views release it and start from today.
Launcher reconstructs Search/Actions mode from its saved route, without queries.
Actual-widget regressions reproduce the old failures and pass with the fixes.

| Check / UI MiB | Hyprland | niri |
| --- | ---: | ---: |
| One-pair Source settled, before → candidate |23.110 → 22.833|23.780 → 23.785|
| Open calendar, Bead / actual QML calendar+header |28.486 /195.153|31.200 /176.293|
| Workspace tests / trace proof / fmt / clippy / CPU-only |197 /pass /pass /pass /pass|same source|

Source memory screen is one matched pair/compositor, not five-pair acceptance;
its deltas fit the previous control spreads. No settled saving is claimed.
Normal immutable binaries/matching bundles, frozen75157 daemon, 320 apps.
Nested CPU scale1.5, Hvirtual120/N60, refresh unknown; host uncontrolled,
no own build/capture overlap. Strict close/cache/group/cleanup guards pass.
The 60-second calendar comparison uses actual QML widgets at matched geometry;
QML static service fixtures and Bead running services differ, so CPU is not a
rendering-speed comparison. Header fixture data/shadow differ; not full parity.
Source snapshot predates the later launcher-restoration fix; no relabeling.
Final binary native Search/Actions restore passes H/N; settled23.050/23.749MiB.
All22 closed/settled samples fit bar+5; group/cache/cleanup and25MiB pass.
[Scope, provenance and method](docs/research/feature9-gate-d.md#calendar-source-and-restored-launcher-modes--2026-10-17).

## 2026-10-17 — Closed-panel ownership and large-raster reclamation

Panel modules and assets already load lazily and release with their worker.
The shared launcher scalar mirror now releases after the 2.2-second grace.
An application-only 4 MiB mmap threshold fixes freed large-raster residency.
Five alternating pairs pass the established memory gate on both compositors.

| UI MiB, Hyprland / niri | Before → cleanup | Paired delta / control spread |
| --- | --- | --- |
| Third warm |48.102→38.350 /47.032→45.603|−9.562/1.029; −1.244/0.389|
| Settled bar |22.901→22.954 /23.398→23.490|+0.147/0.300; +0.097/0.735|
| Maximum candidate settled |23.120 /23.924|both below 25|

Normal immutable binaries, matching bundles and frozen 75157 daemon, 320 apps.
Private CPU, scale 1.5, H virtual120/N nested60; not physical, refresh unknown.
No own build/capture overlap; host uncontrolled. All 158 candidate close samples
fit own bar+5; group/caps/cleanup pass. One paired control fails six close budgets;
valid=false/exit1 data is retained, never converted into a pass. Warm ranges vary
(one niri third-open 50.310 MiB); this is a paired-median, not peak, improvement.
Five CPU pairs give UI paired −0.075/+1.364 ms inside spreads 4.153/3.033;
current UI/daemon window is slower. No typing gain or target acceptance claimed.
193 tests, isolated trace allocator proof, fmt/clippy/CPU-only pass. The test
keeps 1.5 MiB scratch on the heap and returns three 5 MiB mappings on drop.
No allocator/font rewrite; explicit allocator options respected. Separate
calendar/source and ranked-prefix drafts remain untested. No new panel yet.
[Audit, ranges and method](docs/research/feature9-gate-d.md#closed-panel-residency-audit--2026-10-17).

## 2026-10-17 — Named service views: measured combined feature cost accepted

One cold catalog selects launcher, sound or calendar on the shared panel lifecycle.
Calendar reads clock/date from the existing service through a small Source.
The local candidate passes headless and private lifecycle checks.
Owner accepts its combined feature cost; the allowance is per explained feature.

| Check | Hyprland | niri |
| --- | ---: | ---: |
| Private service-view open/close cycles |20 passed|20 passed|
| Five-pair settled UI median, control -> candidate MiB |21.990 -> 23.111|22.840 -> 23.604|
| Median paired increase MiB |+1.121|+0.753|
| Every UI close <= own bar+5 / settled UI <=25 MiB |Pass|Pass|
| Workspace tests / fmt / clippy -D warnings / CPU-only |191 / pass / pass / pass|Same local source|

Nested scale 1.5, virtual 120 Hz / nested 60 Hz; feedback refresh unknown.
No own build overlap; host contention uncontrolled. PSS+DRM includes SwapPss.
Both memory variants have a private keyboard before their bar baseline.
Mapped pages and heap dominate the increase; accounted caches differ by about
4.8 KiB. No exclusive allocation cause is proved. No new typing gain is claimed.
[Method, source provenance and decision](docs/research/feature9-gate-d.md#calendar-and-named-views--memory-decision-2026-10-17).

## 2026-10-16 — UI dispatch remains responsive during activation

Extend the actual-provider duplex proof from queries to published menu activation.
The fake daemon withholds the outgoing reply until the incoming dispatch is ACKed.
This tests the concurrency boundary that could otherwise deadlock the UI link.
No production handler, protocol payload or advertised capability changes.

| Private headless proof | Result |
| --- | ---: |
| Held menu.query /menu.activate |Both exact incoming UI ACKs precede replies|
| Workspace tests |176 passed|
| fmt /clippy -D warnings /CPU-only |Passed|

Committed pinned transcripts, actual ui_provider and private UI IPC; no compositor
or live desktop. F3 typed non-app actions/same-engine QML remain upstream, so this
is transport coverage rather than final non-app acceptance. Memory is unchanged.
[Evidence](docs/research/feature9-gate-d.md#outbound-activation-duplex-contract--2026-10-16).

## 2026-10-16 — Reject Lua row cloning; retain launcher pixel coverage

Cloning static row skeletons in Luau has no consistent typing CPU benefit.
The screen uses identical accepted executables, so native code growth is absent.
Keep the runtime unchanged and preserve local5043af7 rather than expand the trial.
Retain the useful accepted-widget pixel/action tests independently.

| One native200-key pair, H /N | Before → prototype |
| --- | ---: |
| UI CPU mean, ms/key |10.696→10.580 /11.214→11.345|
| Presented p99, ms |26.815→26.402 /29.674→34.609|
| Settled UI MiB |21.777→21.776 /22.817→22.811|

Eight cells exit0; all UI-close/35s-group guards pass. A single pair cannot
establish spread or a tail regression.320 apps, same75157 engine, matching
immutable bundles, Hvirtual120/N60,2880×1800/scale1.5; refresh unknown,
contention uncontrolled, no own build overlap. CPU includes background work.
Persistent2,000-call loop: daemon+provider CPU5.715/4.260ms H/N; RTTp99
10.038/8.652ms. Query checks pass; both idle checks fail host paging. No UI
frame claim. Shared method sent upstream; cross-host cause remains unproved.
Three actual-widget goldens cover grid/results/scroll, disabled rows and actions;
bundled fonts and exact pixels. Calendar body remains local, unwired preparation.
[Evidence](docs/research/feature9-gate-d.md#lua-row-cloning--rejected-at-the-screen).

## 2026-10-16 — Native row templates rejected; provider duplex proved

The native template prototype saves about21% UI CPU but still misses8ms/key.
It raises niri warm/settled memory beyond the matched control spread.
Keep the accepted renderer/frontend; preserve4db6682 locally, outside the plan.
The actual UI provider can ACK an incoming call while an outgoing reply is held.

| Five alternating pairs/compositor | H / N |
| --- | ---: |
| UI CPU median before→candidate ms/key |10.792→8.445 /10.736→8.507|
| Presented p99 median before→candidate ms |24.705→23.664 /33.462→31.865|
| Warm paired delta / control spread MiB |+0.555/0.704; +0.559/0.313|
| Settled paired delta / control spread MiB |+0.397/0.842; +0.482/0.211|
| Candidate settled maximum MiB |22.527 /23.455|
| UI close/idle and35s group guards |100/100 and20/20 pass|

40 cells exit0;320 apps,200 keys, fixed75157 helper and matching binary/markup.
Nested Hvirtual120/N60,scale1.5; unknown feedback refresh, contended host,
no own build overlap. CPU windows include background work, not engine-only CPU.
Niri file/anonymous settled deltas+0.339/+0.129MiB; text grew57,636bytes,
insufficient to isolate all residency. Prototype178 tests and required checks pass.
Passing close+5/25MiB does not override pure-optimization spread limits.
Actual provider test uses published dispatch/ACK, same link, private UI socket,
and releases menu reply only after ACK;174 tests/fmt/clippy/CPU-only pass.
Tools6350045 CI38947299625 green. F3 typed actions/QML still pending upstream.
[Evidence](docs/research/feature9-gate-d.md#native-row-templates--rejected-after-five-pairs).
No live/install/release. Desktop plan HOLD; stop after bar service views.




## 2026-10-16 — Recovery/static text accepted for the core route

The owner accepted the small resident increase for real recovery/accessibility features.
The new feature allowance is0.5MiB settled each, with a25MiB UI bar ceiling.
Pure optimizations still preserve warm/settled memory within the control spread.
Continue typing/contracts then bar service views; stop before dashboard.

|Five alternating pairs/compositor|H /N|
| --- | ---: |
|Settled paired increase MiB|+0.339 /+0.392|
|Maximum candidate settled UI MiB|21.817 /22.912|
|Remaining25MiB headroom|3.183 /2.088|
|UI close and35s combined-group ownbar+5|20/20pass|
|Local tests, exact recovered source|173pass,3manualbenchesignored|

Source d9fc758 is unchanged; previous WIP0a77d31 retained. Existing fmt/clippy
all-target/all-feature-Dwarnings/CPU-only/release pass; integration reruns checks.
Strict private H/N presented restart≤77/26ms and2s mark/newboot/provider pass.
Static AccessKit text is headless coverage; no real reader/IME acceptance.
Prior memory stop below is historical and superseded by this owner decision.
Nested Hvirtual120/N60,scale1.5, refreshunknown; host contention uncontrolled,
no own builds overlapped captures. No live/install/release; desktop plan HOLD.
Core40–81eight-hourdays/3–8weeks remains a forecast, to update after service views.

## 2026-10-16 — Gate E candidate: stop for memory decision

Reconnect recovery and static-text accessibility pass functional checks.
Five pairs per compositor show a small but repeatable closed-memory increase.
The owner's separate no-regression rule stops this candidate despite bar+5 guards.
Its code stays local; only evidence and the prepared-only desktop plan are pushed.

|Nested, five alternating pairs/compositor|Before → candidate median MiB|Paired delta / control spread|
| --- | ---: | ---: |
|H third warm|46.789 →47.165|+0.440 /0.099|
|H third closed|21.467 →21.828|+0.360 /0.101|
|H settled closed|21.479 →21.811|+0.339 /0.138|
|N third warm|44.937 →45.284|+0.413 /0.502|
|N third closed|22.335 →22.700|+0.448 /0.240|
|N settled closed|22.381 →22.754|+0.392 /0.293|

Same320-app catalog/regular75157helper; private scale1.5, Hvirtual120/N60,
feedback refresh unknown. No own builds/parallel captures; contention uncontrolled.
Current source d9fc758,173tests/fmt/all-target/all-feature clippy-Dwarnings/
CPU-only/release pass; local only, no candidate CI. Private presented bar restart
≤77/26ms H/N, two-second mark/new-boot/provider registration verified. No real
Orca or whole-menu cold-open claim. Closed file PSS rises while anonymous falls;
retained cache leakage is not established. All20 UI-close/35s-group guards pass;
N warm delta is inside its spread. No guessed allocator/font fix.
Typing UI11.041/10.988ms/key, p9924.287/34.679ms H/N,800 exact keys.
One pair each, no optimization claim; both8ms UI targets fail.
Literal JSON replies25,624–26,227bytes excluding LF. Component daemon2.890/3.152
+provider1.081/1.339ms/query; RTTp998.763/10.606ms,4,000 exact calls.
Helper unchanged; lower repeat cost is not an engine optimization.
Full parity102–207 /core40–81 eight-hour days; calendar7–18 /3–8weeks at explicit
16–20taskh/day scenario. Coreidle90–150MiB estimated, not combined-service proof.
Prepared controller syntax checked, NOT RUN. No live/install/Feature11. Details,
failed attempts, method and forecasts: docs/research/feature9-gate-d.md Gate E.

## 2026-10-16 — Gate E persistent-link component diagnostic

The owner resumed a focused block, with QML still the daily shell.
A persistent link removes the fresh-CLI handshake confounder.
The same-host daemon/provider cost remains above the migration-host result.
This is component CPU and round trip, never presented typing acceptance.

|320 apps,101 results,2,000 calls after100 warmups|H /N|
| --- | ---: |
|Daemon CPU ms/query|3.993 /4.433|
|Provider CPU ms/query|1.572 /1.636|
|RTT p50 /p99 ms|6.218 /11.238;6.550 /11.526|
|Python reencoded bytes (not wire)|25,630–26,233|

Private Hvirtual120/Nnested60,scale1.5; actual refresh unknown. Shared-host
contention uncontrolled; no own builds. Full-loop high-resolution process clocks,
fixed provider population, exact IDs/query/revision/order checked for all4,000.
No UI/keyboard/per-call process or measured memory claim. Existing private Bead
lab reused after upstream lab's absent wlr-randr dependency; no install. Method,
limits and raw artifact pointers: docs/research/feature9-gate-d.md Gate E section.

## 2026-10-16 — Gate D review: adjust

The shared daemon feeds the launcher; Bead keeps rendering and view state.
The bounded rounded repair passes its original memory/pixel gates.
Typing still misses the target, and the production QML daemon adapter is pending.
A private nested preview makes the current bar, launcher and notices reviewable.

|Nested current, H /N|Bead|Matched QML UI-only|
| --- | ---: | ---: |
|Fixture CPU mean ms/key|9.171 /9.040|113.911 /116.460|
|Fixture presented p99 ms|16.311 /21.933|115.939 /102.074|
|Fixture typed-list MiB,60s|37.448 /42.336|574.955 /555.438|
|Real-engine UI CPU mean ms/key|10.045 /9.860|unmeasured|
|Real-engine presented p99 ms|25.425 /35.507|unmeasured|

320-app fixtures,160 changed results; QML keeps its extra details/controls.
One200-key cell each, no full-parity/renderer-only/engine-cost attribution.
Real engine has320 apps,100 app+1action typed results, same regular75157helper.
Daemon/provider CPU means4.973/2.013 H,4.870/2.091 N ms/key include background
in the wider injection→present window; do not add as serial elapsed time.
Real group warm98.006/96.243→settled55.047/55.858MiB; UI21.965/22.854 closed.
UI closes and35s-settled group pass ownbar+5; warm scratch allocations released.
165tests/fmt/clippy all-target/all-feature/CPU-only/release and strict H/N cleanup
pass; early source-thread signal inheritance fixed, caller mask restored.
Lazy-mask f651c04 stays local: warm/closed deltas exceed five-pair control spreads,
CPU neutral/slower. Correct rounded renderer3bf5eff remains, CI38899237836 green.
Nested Hvirtual120/N60,scale1.5, feedback refresh unknown; no own builds,
host contention uncontrolled. Forecast99–201 focused agent-days; hard media,
windows, AI, input/hardware parts dominate uncertainty. STOP at Gate D.
Full [report](docs/research/feature9-gate-d.md#gate-d-review--2026-10-16) includes
current notice/OSD comparison, missing contracts and owner preview limitations.

## 2026-10-16 — Bounded rounded repair kept

Repair-sized scratch fixes exact rounded-corner partial repaint pixels.
It includes the curve guard, reuses capacity and releases after500ms quiet.
Five alternating pairs each fit the unchanged memory gates; no waiver needed.
Niri p99 is higher inside the control spread; typing targets stay open.

|Five-pair medians|Before H /N|Candidate H /N|
| --- | ---: | ---: |
|First-open MiB|50.540 /48.675|50.749 /48.795|
|Third warm MiB|48.098 /46.406|48.325 /46.475|
|Third closed MiB|21.924 /22.812|22.007 /22.942|
|UI CPU ms/key|9.958 /10.643|8.914 /9.883|
|Presented p99 ms|22.083 /20.675|21.965 /25.010|

Warm/closed deltas fit control ranges H0.292/0.245,N0.399/0.323MiB.
Every60 cycle plus20 hover guards pass ownbar+5; first deltas<1MiB.
N p99 +4.335ms fits9.096ms control range, not a claimed latency improvement.
161tests (6,400 exact repairs, scratch reuse/cap/idle), fmt, all-target clippy,
CPU-only and release pass.16MiB admission limit; no eager canvas allocation.
The8MiB prototype failed on a near-full fractional-scale repair; preserved.
Nested Hvirtual120/N60, feedback refresh unknown; no own build overlap,
shared-host contention uncontrolled. Raw `bounded-scratch-fivepairs/`.
Full method/results: [Gate D report](docs/research/feature9-gate-d.md#bounded-rounded-repair--2026-10-16).

## 2026-10-16 — Stop: exact rounded repair exceeds memory budget

The expanded pixel test found a baseline antialias mismatch in cropped repairs.
Full-canvas rounded geometry plus masking fixes it; no tolerance was widened.
Conservative shortcuts pass the exact corpus but add too much anonymous memory.
The candidate is preserved locally and rejected from main under the owner's stop rule.

|First A/B pair, MiB|Before H /N|Candidate H /N|
| --- | ---: | ---: |
|First open|50.909 /48.638|51.885 /50.109|
|Third warm|48.515 /46.538|49.639 /47.769|
|Third closed|22.310 /22.950|22.121 /23.075|
|UI CPU ms/key|9.887 /9.581|9.536 /9.514|

160 tests/fmt/clippy ALL targets/CPU-only/release pass. N first+1.471MiB and
warm+1.231 violate the standing rule; second H pair first+1.195/warm+1.220.
N SHM stays7.455MiB; anonymous first+1.230/warm+0.992 points to full-canvas
mask/allocator retention. This attribution is an inference, not allocation tracing.
All completed close guards pass bar+5, without waiving the steady-memory limit.
Continuation stopped, partial evidence retained, private processes cleaned up.
Local WIP7271a86 is not pushed; accepted code/binary4d9aa45 restored. No larger
run or performance acceptance is claimed. Nested Hvirtual120/N60,scale1.5,
unknown refresh predictor,shared-host contention uncontrolled,no own build overlap.
[Evidence](docs/research/feature9-gate-d.md#rounded-repair-memory-stop--2026-10-16).

## 2026-10-16 — Native Luau value constructors

Brushes, binding markers and action records now use Luau data constructors.
Existing coercions, shared arguments, nil/false and caller diagnostics are preserved.
Five pairs show CPU wins on both compositors; five memory pairs fit control spreads.
Both typing budgets remain unaccepted; no end-to-end gain is inferred from fixtures.

|5 typing-pair medians|Before H /N|Candidate H /N|
| --- | ---: | ---: |
|UI CPU ms/key|11.243 /11.309|10.727 /10.695|
|Presented p99 ms|22.514 /24.149|22.360 /21.145|
|Third warm MiB,5 lifecycle pairs|47.414 /45.558|47.456 /45.569|
|Third closed MiB|21.080 /21.974|21.200 /21.950|

159 tests/fmt/clippy ALL targets/CPU-only/release,4000 key proofs and60 lifecycle
cycle plus hover guards pass. H warm/closed deltas0.042/0.120 fit ranges0.347/
0.183;N0.012/−0.023 fit0.221/0.200. First-open+0.038/+0.013MiB; no waiver.
No new retained cache; existing compiled-markup cap accounts for SDK bytes.
Fresh private Hvirtual120/N60,scale1.5,unknown refresh predictor,no own builds;
external contention uncontrolled. Decoder profiling and keyboard-batch diagnostic
are separate: no parser replacement or input-method speedup is claimed. Exact
native comparison on the same published helper is recorded separately.
[Method](docs/research/feature9-gate-d.md#native-luau-value-constructors--2026-10-16).

## 2026-10-16 — Drop static property-map borrowing

The copy-on-write prototype preserved rendered pixels and all existing behavior checks.
It did not deliver a CPU median win on both compositors over five alternating pairs.
The patch is saved and reversed; the independent catalog-selection fix remains.
No cache, ranking or provider change is accepted from this experiment.

|5 typing-pair medians|Before H /N|Candidate H /N|
| --- | ---: | ---: |
|UI CPU ms/key|9.980 /9.881|9.888 /10.664|
|Presented p99 ms|22.282 /19.694|22.246 /20.938|

H paired CPU−0.244ms within0.682 control range; N+0.288 within1.527.
No repeated total CPU benefit on both; tail deltas fit control spreads.4000 exact
key proofs and the first lifecycle pairs pass,157 tests/fmt/clippy ALL targets/
CPU-only/release pass. New fallback-selection test covers catalog shrink without
query edit. Fresh private Hvirtual120/N60,scale1.5,unknown refresh prediction;
no own build overlap; external contention uncontrolled. No physical acceptance.
[Method](docs/research/feature9-gate-d.md#rejected-static-property-borrowing--2026-10-16).

## 2026-10-16 — Real engine Source, catalog refresh and separate CPU clocks

The launcher now observes daemon menu generations and refreshes without a key.
Empty service wakeups consult projection; only retryable busy reads retry within3s.
Activation never retries. Native typing and idle-release proofs pass on both compositors.
The typing budgets fail; same-engine QML comparison remains unavailable until F3.

|3 runs×200 keys, medians|Hyprland|niri|
| --- | ---: | ---: |
|UI CPU ms/key|12.230|12.047|
|Daemon CPU ms/key-window|8.332|8.101|
|Provider CPU ms/key-window|1.944|1.856|
|Presented p99 ms|31.559|30.360|
|Real lifecycle warm UI+external MiB|96.962|95.846|
|35s settled UI+external MiB|53.185|55.115|
|Matched bar group MiB|51.620|54.498|

1200 exact RPC/typed-result/commit/presentation proofs pass; no timed busy retry.
156 tests/fmt/clippy ALL targets/CPU-only/release and five canonical transcript
families pass. Native catalog invalidation and owned-worker cleanup pass. H10/N5
fixture memory pairs: first/warm/closed deltas fit control spreads;90 cycle and
hover guards pass ownbar+5. Original borderline H5 set retained, one fixed
confirmation only. Native settled combined groups also pass bar+5 with SwapPss.
32s daemon settling,320 apps/101 results, actual nested backend,scale1.5;
Hvirtual120/N60 configured, feedback refresh prediction unknown. No own builds;
shared-host contention uncontrolled. Daemon/provider clocks include background
work in the injection→observed-feedback window, not pure RPC CPU. Detailed
API/idle/stage diagnostic is separate from performance acceptance. Prior failed
catalog/busy/cleanup runs remain; migration is coalescing background metadata
loads. No helper implementation, physical/live test, install or release change.
[Method](docs/research/feature9-gate-d.md#real-engine-source-and-refresh-checkpoint--2026-10-16).

## 2026-10-16 — Drop duplicate bake serialization/copy prototype

The bake prototype passed pixel, clip, pose and raster-reuse correctness checks.
Its first-pair CPU gain did not survive five alternating pairs on both compositors.
The patch is saved for audit and reversed; it is absent from accepted source.
Memory fell in a separate pair, but that does not establish the intended CPU win.

|5 typing-pair medians|Before H /N|Candidate H /N|
| --- | ---: | ---: |
|UI CPU ms/key|9.719 /10.200|10.030 /10.146|
|Presented p99 ms|22.466 /18.560|21.896 /21.561|

H paired CPU+0.513ms; N−0.059 inside control spread0.446. N tail+3.001ms
fits control spread3.465, but no robust CPU benefit on both compositors.4000
exact key proofs pass. fmt/clippy ALL targets/153 tests/CPU-only/release pass;
the initial CPU-check command used a nonexistent feature, then the corrected
no-default-features command passed. Fresh private Hvirtual120/N60,scale1.5,
unknown refresh prediction; no own build overlap, external contention uncontrolled.
[Method](docs/research/feature9-gate-d.md#rejected-duplicate-bake-serializationcopies--2026-10-16).

## 2026-10-16 — Launcher input parity over ordered engine results

Query edits select the first enabled result and reset scroll; arrows use layout geometry.
The empty apps grid starts unselected; disabled boundaries and activation errors are tested.
This is correctness work, with no measured typing improvement claimed.
Memory deltas fit the five-pair control ranges; all close/hover guards pass.

|5 lifecycle-pair medians|Before H /N|Candidate H /N|
| --- | ---: | ---: |
|First-open MiB|50.748 /48.464|51.082 /48.792|
|Third-warm MiB|48.171 /46.146|48.567 /46.367|
|Third-closed MiB|21.923 /22.582|22.187 /22.768|
|Closed control range width MiB|1.482 /0.953|—|

153 tests/fmt/clippy ALL targets/CPU-only/release pass. Nested input proof13
states each:12 exact presentations plus one unchanged-boundary acknowledgment.
60 cycle checks and hover guards pass bar+5MiB. Mixed typing results and full
spreads are in the linked report; neither typing budget is accepted. No real
engine or physical measurement. SDK optlevel2 rejected; SDK level1 retained.
Fresh private Hvirtual120/N60,scale1.5; refresh prediction unknown, no own build
overlap; external contention uncontrolled. No closed/steady threshold waiver.
[Method](docs/research/feature9-gate-d.md#launcher-input-parity--2026-10-16).

## 2026-10-16 — Native copies for plain constructor properties

The plain-table constructor path uses Luau's native clone instead of per-key copies.
It preserves caller tables, strict validation, protected metatables and diagnostics.
Five pairs show a Hyprland CPU win; niri CPU is neutral within the measured spread.
No typing-budget or physical acceptance is claimed.

|5 typing-pair medians |Before H /N |Candidate H /N |
| --- | ---: | ---: |
|UI CPU ms/key |10.464 /10.012|9.658 /10.061|
|Presented p99 ms |22.383 /19.278|22.075 /18.479|
|Third closed MiB, separate lifecycle pair |22.282 /23.096|21.973 /23.062|

151 tests/fmt/clippy ALL targets/CPU-only/release pass.4000 key proofs and all
lifecycle guards pass. H paired CPU−0.926ms; N marginal+0.049/paired−0.027,
baseline range1.033. Do not claim a measured N CPU gain. First/warm memory falls.
Fresh private configured Hvirtual120/N60,scale1.5; refresh prediction unknown,
no own build overlap, external contention uncontrolled. No physical/engine proof.
Later keyboard-selection work is separate.
[Method](docs/research/feature9-gate-d.md#plain-constructor-property-copies--2026-10-16).

## 2026-10-16 — Reuse existing font state for caret metrics

Caret metrics now borrow the bake renderer's existing font system.
The bounded weak index owns no font data; renderer teardown still releases it.
Five typing and lifecycle pairs each show CPU wins and no memory regression.
Niri's marginal CPU gain is small; the typing budgets remain unaccepted.

|Per-run medians,5 pairs |Before H /N |Candidate H /N |
| --- | ---: | ---: |
|UI CPU ms/key |10.638 /11.311|10.095 /11.137|
|Presented p99 ms |22.271 /20.732|21.903 /19.742|
|Third warm MiB |48.297 /46.571|48.285 /46.401|
|Third closed MiB |22.029 /22.964|21.943 /22.863|
|Font loads per key |1 /1|0 /0|

150 tests/fmt/clippy ALL targets/CPU-only/release pass. Exact fresh-font advance,
teardown and file-stamp invalidation checks pass; weak-index report1304 bytes.
Numeric RPC/typed-result readiness and wrong-query rejection are included; the
real daemon engine is still missing. Later constructor-copy draft is excluded.
Private320-app fixtures, clock-only, no own build overlap; external contention
uncontrolled. Refresh predictor unknown, not physical acceptance. All lifecycle
checks stay strict; no closed/steady memory waiver.
[Method and limits](docs/research/feature9-gate-d.md#borrow-existing-font-state-for-caret-metrics--2026-10-16).

## 2026-10-16 — Keep clocked UI typing improvements

Matched clock-only runs confirm the UI CPU median win on both compositors.
Ordered pixels and separate SHM repair avoid broad repaints; Luau builds fewer
native objects and result models share immutable storage.
Ten lifecycle pairs each show lower first/steady/closed memory, without a waiver.
The8ms CPU and16.7ms presentation targets remain unaccepted.

|Per-run medians |Before H /N |Candidate H /N |
| --- | ---: | ---: |
|UI CPU ms/key,5 pairs |15.467 /16.957|10.099 /10.830|
|Presented p99 ms,5 pairs |24.715 /27.019|22.191 /21.338|
|First warm MiB,10 pairs |50.796 /48.296|50.224 /47.623|
|Third warm MiB,10 pairs |48.510 /46.197|47.949 /45.604|
|Third closed MiB,10 pairs |21.867 /22.292|21.638 /22.026|

147 tests/fmt/clippy/CPU-only/release pass.4000 key proofs/160 current lifecycle
checks pass; one initial N private IPC failure is retained, exact retest and20
extra lifecycle sessions pass. Failure reply capture is added; cause unclassified.
Fresh private Hvirtual120/N60 configured,scale1.5; feedback refresh unknown.
No own heavy build overlap; external contention uncontrolled. No physical/engine
acceptance. Later font/RPC drafts are excluded. Native GL functional smoke passes (6 keys each).
[Method, spreads and limits](docs/research/feature9-gate-d.md#clocked-ui-typing-improvements--2026-10-16).

## 2026-10-16 — Separate typing clocks from detailed stage tracing

Detailed tracing adds observation work, especially with several paint regions.
Timestamp-only mode retains the exact presentation proof and whole-process CPU.
Unknown refresh predictions remain missing data, not an asserted zero rate.
The same-binary calibration is not an optimization acceptance.

|Calibration,64 keys/run,2 runs/mode/compositor |H |N |
| --- | ---: | ---: |
|Detailed CPU median ms/key |12.099|13.254|
|Clocks-only CPU median ms/key |10.187|11.753|
|Clocks-only range ms/key |8.797–11.576|11.701–11.805|
|Exact key proofs /unknown refresh predictions |256 /256|256 /256|

147 local candidate tests/fmt/clippy/CPU-only/release pass; this checkpoint
publishes only measurement infrastructure/docs. Default detailed mode is retained.
Private Hvirtual120/Nnested60 configured, scale1.5; no own build overlap, external
contention uncontrolled. Neither configured refresh is verified by feedback; no
physical test. UI8ms/presentation16.7ms and real-engine comparisons remain open.
[Method](docs/research/feature9-gate-d.md#typing-clocks-and-tracing--2026-10-16).

## 2026-10-16 — canonical menu payloads reach the fixture Source

The pinned Nacre contract replaces the interim row-array transport shape.
Entity order stays engine-owned; activation descriptors/revision stay outside Luau.
Native and unchanged QML private smokes consume the same canonical records.
This accepts interface compatibility, not the real engine or typing targets.

|Check | Result |
| --- | --- |
|Pinned protocol /committed menu transcript families |3042f3f2 /4|
|Isolated workspace /fmt /clippy /release |141 pass /pass /pass /pass|
|CPU-only /Luau /service /permissive graphs |pass /23 modules /pass /pass|
|Native input /QML input H,N |12,12 /6,6 exact presentations|
|Bar→third closed PSS+DRM MiB H /N |22.115→22.132 /23.541→22.916|

Three cycles, hover expiry, query RAM, geometry and cleanup pass both; close<=bar+5.
H virtual120/N nested60, fresh private sessions, no own heavy build overlap;
external load/paging uncontrolled. Short smokes are not timing/idle acceptance.
Upstream full CI hit its pre-existing Pillow/helper parity failure; migration
owns its83a9a9a4 repair. Protocol11tests/export pass; no helper changes here.
[Method and limits](docs/research/feature9-gate-d.md#canonical-menu-source--2026-10-16).

## 2026-10-16 — keep count-only after five alternating pairs

Five pairs per compositor confirm the median CPU benefit on a consistent workload.
Presented p99 medians improve; closed and steady deltas fit baseline run spread.
All4000 key presentations and80 close/unused-hover checks pass without loosening.
The count-only API is kept; both typing budgets remain unaccepted.

|Five trial medians | Before H /N | Candidate H /N | Baseline range width H /N |
| --- | ---: | ---: | ---: |
|UI CPU mean ms/key |17.459 /19.973|16.160 /17.775|3.175 /2.583|
|Presented p99 ms |30.296 /29.822|25.129 /27.198|7.243 /3.957|
|Third warm PSS+DRM MiB |48.059 /46.397|48.238 /46.432|0.391 /0.586|
|Third closed PSS+DRM MiB |21.544 /22.638|21.788 /22.692|0.547 /0.638|

Paired CPU medians−1.830/−1.446ms; closed deltas+.244/+.055MiB fit the band.
Every close <=bar+5 incl swap; first-warm median change−.006/+.033MiB.
H virtual120/N nested60,320apps/disjoint160-row exact replies,200keys/sample.
Fresh sessions, no own heavy build overlap; shared-host load/paging uncontrolled.
137tests/fmt/clippy/Luau pass. Canonical menu contract3042f3f2 is now upstream;
engine/end-to-end CPU still unmeasured. No physical/full-parity/typing acceptance.
[Method and limitations](docs/research/feature9-gate-d.md#count-only-decision-five-alternating-pairs--2026-10-16).

## 2026-10-16 — count-only API saved; mixed tails remain unresolved

Directly reading model length reduces Luau preparation and median UI CPU.
The candidate still misses both typing budgets; niri's short-run tails are mixed.
Small positive steady/closed PSS also needs confirmation before integration.
The tested candidate is preserved locally; main keeps the accepted cropped path.

|UI CPU mean ms/key, three trial median | Before H /N | Candidate H /N |
| --- | ---: | ---: |
|One 200-key plus two alternating 80-key pairs |18.398 /20.326|16.083 /16.740|
|Presented p99 trial range ms |31.61–38.67 /27.40–30.07|24.82–30.66 /26.44–38.10|
|Third closed PSS+DRM MiB, one pair |21.782 /22.825|21.958 /22.887|

All lifecycle closes pass bar+5 incl swap; no tolerance relaxed.
H closed+0.176MiB: file-backed+0.164/anonymous+0.012; ELF+3,072 bytes.
No causal pacing/leak conclusion from uncontrolled single lifecycle pairs.
H virtual120/N nested60;320 apps/disjoint 160-row exact replies, no helper engine.
137 tests/fmt/clippy/release/Luau pass; local 95862ca held, not main/CI accepted.
Upstream F2 priority gate now blocks real-engine work; no Gate D acceptance.
[Methods and limits](docs/research/feature9-gate-d.md#count-only-model-access--measured-candidate-held-2026-10-16).

## 2026-10-16 — crop transparent text padding without changing pixels

Text rasters retained the whole layout width; blending transparent pixels was costly.
Store only ink and an integer origin, applied after the original position rounds.
CPU/native GL and damage use that origin; layout, shaping and font handling stay intact.
Matched pairs reduce composition and memory; typing targets remain unaccepted.

|200 changed-result keys, nested | Before H /N | Cropped H /N |
| --- | ---: | ---: |
|UI CPU mean ms/key |24.946 /25.691|18.454 /18.774|
|Presented p99 ms |33.525 /41.354|30.426 /29.015|
|Typed-list PSS+DRM MiB,3s idle |42.517 /46.243|38.821 /42.459|
|Text composition CPU ms/key |6.799 /7.808|0.560 /0.617|
|Whole composition CPU ms/key |12.314 /13.990|6.229 /6.550|

Active-view bundle, same numeric spans;320 apps/disjoint 160-row exact replies.
H virtual120/N nested60,2880×1800/scale1.5; no own build overlap, external load uncontrolled.
Single sequential pair each;3s memory diagnostic, not idle invariance/physical proof.
136tests/fmt/clippy/release and fractional/clip/alpha/moving-ink pixel checks pass.
Lifecycle guards pass both; third warm H51.20→48.42/N50.21→46.50MiB.
Every close <=bar+5 incl swap; first/warm/closed show no matched regression.
Native-GL grid/list feedback/cleanup smokes pass both. CPU8ms/presented16.7ms
budgets still fail; helper CPU/end-to-end unmeasured.
[Full method](docs/research/feature9-gate-d.md#tight-text-rasters--2026-10-16).

## 2026-10-16 — changed launcher results expose hidden-view work

Identical a/ap rows measured a best-case reuse path, not real result changes.
A second exact fixture swaps disjoint 160-row sets from320 total apps each edit.
The launcher built a hidden grid as well as the list; only the active view is now built.
Preparation improves, but changed-result typing still misses both budgets.

|200 keys, nested fixture | Before H /N | Candidate H /N | QML H /N |
| --- | ---: | ---: | ---: |
|UI CPU mean ms/key |36.568 /35.595|23.897 /26.059|133.780 /135.558|
|Presented p99 ms |48.053 /50.516|38.966 /40.492|110.741 /111.242|
|Typed-list PSS+DRM MiB,3s idle |43.72 /47.50|42.42 /46.44|502.09 /488.93|

H virtual120/N nested60,2880×1800/scale1.5, exact200 feedback generations each.
Same30804a7 release, candidate bundle only; unchanged QML retains extra controls.
Sequential fresh sessions; no own build, migration diagnostic/external load uncontrolled.
H markup CPU16.22→5.95ms; composition11.85→12.42ms remains expensive.
No engine/ranking/helper CPU; no physical/full-parity/timing acceptance.
5a7f548 docs-only CI repeated the50ms guard failure; limit stays unchanged.
See [method and limitations](docs/research/feature9-gate-d.md#changed-result-typing-and-active-view-preparation--2026-10-16).

## 2026-10-16 — preserve validation while fixing CI markup timeout

CI caught the320-app headless view crossing the unchanged50ms markup limit.
Component checks made needless Rust calls for ordinary row scalars and records.
The fast path still resolves bindings and rejects unknown/wrong nested properties.
Error paths are assembled on failure; no execution limit or fixture was reduced.

| Local debug diagnostic, initial populated markup | Before | Candidate |
| --- | ---: | ---: |
|Lua evaluation ms |37.40|25.00|
|Whole markup ms |51.49|39.75|
|Nested error path /valid bound string /invalid bound number |N/A|pass|
|Workspace /fmt /all-features clippy |135 /pass /pass|pass|

Different diagnostic executions, not a release CPU or timing-gate acceptance.
CI38825928964 failure is retained; corrective e2ba01d /CI38827038558 is green.

## 2026-10-16 — unchanged QML launcher view versus Source fixtures

Both views receive320 ordered app rows, icons, labels and a/ap queries.
No catalog, provider or ranking engine runs in either fixture process.
Real key receipt/query commit/compositor feedback match200 generations each.
Memory differs greatly; warm CPU is close and cold/engine gates remain open.

|Nested UI-only fixture | Bead H /N | QML H /N |
| --- | ---: | ---: |
|UI CPU mean ms/key |5.270 /5.723|5.689 /5.548|
|Presented p99 ms,200 keys |15.091 /15.828|15.743 /12.697|
|Typed-list PSS+DRM MiB after60s |37.804 /41.812|253.516 /208.511|
|Observed idle CPU % |0 /0|0.467 /0.467|
|Helper CPU /real engine |unmeasured|unmeasured|

H virtual120/N nested60,2880×1800/scale1.5; one edit in flight. Source clock1,
whole UI-process CPU, injector excluded; persistent private virtual keyboard.
QML retains details/quick controls/filter/hints; Bead subset lacks that polish.
Both show real populated rows; no blank-surface/full-parity comparison claimed.
QML normal focused input/extra controls affect idle;0ticks limited by10ms timer.
No own build overlap, unrelated load/frequency uncontrolled; no SwapPss.
Native noDRM; QML includes81.12/49.83MiB actual DRM. First-transition maximums
remain recorded separately; physical120Hz and real engine targets unaccepted.
[Method and scope](docs/research/feature9-gate-d.md#matched-launcher-view-comparison--2026-10-16).

## 2026-10-16 — Source launcher fixtures and measured framework fixes

The view forwards queries to a Source; Nacre owns catalog/ranking/providers.
Immutable list snapshots avoid copying a populated model for a character edit.
Correct output-based layout removes a recursive resize/idle-memory failure.
Typing improves, but cold transitions and the overall niri tail remain open.

| Nested default CPU,320 exact fixture rows | H120 | N60 |
| --- | ---: | ---: |
|200 keys, UI CPU mean ms/key |5.093|5.010|
|All /steady presented p99 ms |13.959 /13.884|19.057 /16.485|
|First key CPU /presented ms |31.307 /39.059|24.572 /28.931|
|Same40-key steady diagnostics, before→shared models |8.30→4.15|10.62→5.49|
|Launcher bar /first warm /third closed MiB |22.17 /53.07 /23.45|23.53 /51.25 /24.39|
|Sound bar /first warm /closed5s MiB |22.07 /28.89 /22.78|23.62 /30.92 /23.68|
|Helper CPU /real engine typing |unmeasured|unmeasured|

One edit in flight, first a then a/ap populated list; no own heavy build overlap,
external load/frequency uncontrolled. First key included and reported separately.
Do not compare54ms initial grid/list-toggle CPU directly with5ms steady typing.
All lifecycle closes within ownbar+5; current sound below published-Rust control.
No allocator policy kept. Direct opaque composition now wins the repeated1c
comparison: bar1.623/1.778ms direct/cached, sound0.586/1.683; alpha cache retained.
Pixel/counter/transaction/budget tests pass;134workspace/fmt/clippy/CPU+release,
Luau23modules/service/permissive-framework/license checks pass. Same cumulative
50ms behaviour allowance excludes native/other-VM idle time; tests prevent resets.
[Method and limits](docs/research/feature9-gate-d.md#feature-10-source-fixture-checkpoint--2026-10-16).
Gate D, canonical engine/prefixes and matched QML launcher are not accepted yet.

## 2026-10-15 — populated notification and OSD comparison

The view reads notifications through a Source; service behavior stays outside it.
Matched visual fixtures avoid taking any live notification name or audio action.
Corrected same-SVG notice captures replace rejected missing-icon trials.
Bead uses much less memory here, but its observed idle CPU is higher.

| 60 s PSS+DRM MiB | Bead H /N | QML H /N |
| --- | ---: | ---: |
| Expanded notice |20.779 /21.588|161.223 /151.207|
| Volume75 OSD |22.371 /23.797|125.090 /121.978|
| Observed CPU % notice |0.217 /0.200|0 ticks /0 ticks|
| Observed CPU % OSD |0.250 /0.217|0 ticks /0 ticks|
| Real private-bus behavior/hover/legacy/four OSDs/cleanup |pass both|not server-compared|

H virtual120/N nested60,2880×1800/scale1.5; one sequential sample/cell after3 s
settling, no own/migration heavy local build overlap; unrelated load uncontrolled.
No SwapPss; Bead no DRM client. QML includes actual DRM. Zero ticks is limited by
10 ms process-tick resolution. Bead includes transitional private bus/audio tasks;
static fixtures host no notification server. Source/controls123tests and CI green.
History remains256 records/3MiB; daemon30-day history stays upstream. Regular
summary weight, rich images/swipe/row-height polish and matched QML presentation
timing remain open. [Method](docs/research/feature9-gate-d.md#feature-9-source-ui-comparison--2026-10-15).

## 2026-10-15 — daemon Source reaches a headless Luau view

The adapter consumes the published protocol; framework Sources stay neutral.
It retains the last good snapshot across gaps/restarts and resubscribes.
A private standalone daemon supplies real compositor state on both compositors.
These cold probes include startup and rendering, not RPC or typing acceptance.

| Check | Result |
| --- | --- |
| Pinned SDK |6bdc22e; unchanged v1 wire contract|
| Golden families / adapter tests |7 /9|
| Cold Source/UI probe ms, H120 /N60 |40.30,37.88 /48.57,42.47|
| Fresh pixels/AccessKit, two boot IDs, cleanup |pass on both|
| Workspace tests / fmt / clippy / CPU+release |123 /pass /pass /pass|
| Framework license and headless-service graphs |pass|

Explicit private backend selection corrects the inherited desktop-selector setup
failure; strict freshness remains. No live endpoint, installation or release.
Feature9 Source fixtures/controls are prepared; matched QML samples pending.
See [adapter method](docs/research/foundation-f5.md#nacre-source-adapter--2026-10-15).

## 2026-10-15 — opaque motion chooses a cached layer, releases it at rest

The opaque comparison uses actual opacity1 and native content-sized surfaces.
Correcting the generic headless bar inset changed its winner; preliminary data
was rejected. Cached motion is cheaper for both populated scenes.
Stable frames stay direct and release the group; no persistent idle cache added.

| Release CPU ms/frame | Direct | Cached |
| --- | ---: | ---: |
| Bar median trial mean |2.100|1.776|
| Sound median trial mean |6.098|3.535|
| Median trial CPU p99, bar /sound |2.474 /6.212|2.085 /3.667|
| Mean CPU reduction, bar /sound |N/A|15.4% /42.0%|
| Native H bar/warm/closed5s PSS+DRM MiB |N/A|20.214/27.139/21.549|
| Native N bar/warm/closed5s PSS+DRM MiB |N/A|21.834/29.202/22.139|

2880×1800/1.5 output; targets2880×75 and696×525. Five alternating trials,
32 warm+240 measured poses each; reused buffers, process CPU around paint only.
Headless CPU, not physical/presentation/typing acceptance. Native smoke contended.
Scale1.5 counter:120 vertical+120 horizontal poses0 layout/new rasters after
priming; pixels≤2 channel delta; idle release and resumed group rebuild proven.
112 tests/fmt/all-features clippy/CPU+release builds pass; all closes<=bar+5.
Verdict: retain cached motion with direct idle policy; proceed to notifications.
[Full method and ranges](docs/research/foundation-f5.md#opaque-composition-follow-up).

## 2026-10-15 — cache accepted; five alternating pairs classify first-open noise

The owner accepted the cache checkpoint and bounded diagnosis to about one hour.
Five samples per variant/compositor plus source audit took about14 minutes.
No large eager allocation was found; logical cache limits do not reserve memory.
Both first-open median deltas lie inside spread; no speculative fix was kept.

| Nested PSS+DRM MiB | Hypr120 baseline→candidate | niri60 baseline→candidate |
| --- | ---: | ---: |
| First-open median |27.451→27.421|29.855→30.065|
| First-open larger within-group range |1.259|1.518|
| Third-open median |27.688→26.506|29.446→28.375|
| Closed5s median |22.111→22.304|22.723→22.928|
| Closed larger within-group range |0.993|0.257|
| All close/lifecycle within own bar+5 |10/10 clients|10/10 clients|

Same compositor, fresh state/cache, AB alternation (reversed N), nice≥10.
Contended, three cycles/five-second idle; not physical, QML or CPU acceptance.
Anonymous first-open medians−8 KiB each; executable-backed+148/+156 KiB.
ELFtext+92336 bytes/rodata+448; registry/LRU maps start empty, no cap-sized reserve.
Small code/metadata and variable buffer/allocator residency remain known costs.
Verdict: noise under owner's spread criterion; move to opaque CPU measurement.
[Full method/ranges and accepted policy](docs/research/cache-budget.md).

## 2026-10-15 — F5 cache candidate held for a memory decision

Recovered the interrupted cache work and verified it before continuing.
One process ceiling and per-owner limits now account retained cache bytes.
First-open memory rose in both final matched pairs; later opens became smaller.
The owner's regression stop rule applies: keep this candidate unintegrated.

| Nested CPU bar + sound, PSS+DRM MiB | Hypr120 before→after | niri60 before→after |
| --- | ---: | ---: |
| Bar |21.834→21.902|23.509→23.629|
| First warm |27.848→28.482|30.854→31.259|
| Third warm |28.035→26.741|30.101→29.382|
| Closed5s |22.602→22.986|23.077→23.761|
| Every close/lifecycle within own bar+5 |pass|pass|
| Native report total / ceiling bytes |188897 /201326592|N/A: separate report proof on H|

Same compositor per pair, fresh private state/cache, three cycles, nice≥10.
Contended short samples; not physical, QML, CPU or idle-invariance acceptance.
111 tests, fmt, all-features clippy, CPU/release builds, deny and Luau pass.
Dependency-private font/driver caches are reset on idle/close, not measured zero.
See [the full accounting and recovery report](docs/research/cache-budget.md).
Verdict: STOP pending first-open memory tradeoff decision; opaque/9/10 remain.

## 2026-10-15 — F5 gap: OSD text bounds

The scale1 OSD label claimed the entire row width and pushed its status outside.
Giving the growing label a zero basis leaves room for the fixed-width status.
The populated golden is reviewed; a bounds/non-overlap assertion guards the cause.
This headless layout fix is not a new native memory or presentation measurement.

| Check | Result |
| --- | --- |
| Populated notice/OSD goldens and pose counter | pass |
| OSD text within panel / label before status | pass |
| Native timing or PSS change | N/A: no renderer/service change |

Verdict: keep the scoped layout correction; cache accounting is next.

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
CI38439776660 passed; raw .research/retained-comparison. [Preview](examples/retained-renderers.png).
Verdict: **adjust — choose femtovg + small retained surfaces; keep CPU fallback**.
No physical120Hz pass claimed; Vello's first/large-surface costs reject it here. STOP.

## Feature 3 — stateful hot reload + shared native GL (2026-10-13)

Typed JSON/Luau reloads preserve stable-ID input, scroll, focus and open panels.
All UI sources/assets, bindings, palette and GPU uploads stage before one swap;
broken files keep the old UI with file/line error overlay, and fixing clears it.
Native GL is lightest and matched QML physical120Hz; CPU remains the no-GPU fallback.
Cold startup misses QML, and reload bakes still pause the UI thread ~80–103ms.

| Variant/run | CPU % | Draw p50/p99 ms | Callback p50/p99 ms | PSS + DRM total (PSS/USS + DRM), MiB | Startup ms |
| --- | ---: | ---: | ---: | --- | ---: |
| GL/first | 2.62 | .048/.153 | 8.341/9.383 | 90.56 (46.48/43.45 +44.08) | 268.88 |
| GL/later | 5.23 | .061/**29.690** | 8.366/**102.434** | 93.03 (32.98/30.82 +60.05) | 112.49 |
| wgpu default/first | 4.55 | .210/.437 | 8.331/9.341 | 166.97 (45.92/43.92 +121.05) | 197.04 |
| wgpu default/later | 4.67 | .221/.479 | 8.334/9.809 | 159.77 (38.72/37.27 +121.05) | 410.72 |
| wgpu MemoryUsage/first | 4.17 | .269/.549 | 8.400/71.810 | 130.34 (45.49/43.90 +84.84) | 185.37 |
| wgpu MemoryUsage/later | 4.43 | .265/.570 | 8.382/36.222 | 123.55 (38.71/37.45 +84.84) | 97.94 |
| GL/3 surfaces/first, later | 6.25,13.60 | — | — | 144.09,132.09 (45.65/43.60 +98.44; 33.53/31.40 +98.56) | 373.88,139.68 |
| UI GL idle/first, later | 0,0 | static | static | 101.29,87.29 (51.27/46.80 +50.02; 37.52/33.38 +49.77) | 314.89,108.01 |
| UI CPU idle/first, later | 0,0 | static | static | 23.72,23.32 (23.72/20.03 +0; 23.32/19.78 +0) | 129.39,81.67 |
| UI QML idle/first, later | 0,.017 | static | static | 200.81,194.31 (104.99/84.79 +95.82; 98.46/75.13 +95.85) | 228.35,162.31 |
| Live GL | 1.51 | .041/.159 | 8.322/9.989 | 95.64 (51.62/46.31 +44.02) | 233.58 |
| Live QML | 3.29 | not measured | not measured | 187.89 (97.98/73.61 +89.90) | 176.04 |

Private60s rows: nested virtual2880×1800/1.5/120Hz, content1746×978; first fresh private caches, OS caches uncontrolled. GL20s repeats callback p99 9.53/9.24ms do not erase the bad60s run. Shared device/context ID0 only; same-scale layer built/uploaded once for1/3 surfaces; peak3-surface DRM144.65/146.66MiB. No hidden per-surface devices.
Live: ONE paired60s niri episode, real2880×1800/1.5/120Hz; CPU55s after5s warmup. Both physical presentation p50/p99 **8.333/8.337ms**, refresh8333313ns, flags7; discarded GL21/QML22 updates. Both stopped; Nacre untouched. Paired GPU load; no additional live authorization.
Latest five-edit reload proof: wgpu/niri78.85–82.99ms, GL/Hyprland99.92–102.71ms, CPU fallback/niri82.06–83.66ms; matched QML50.01–109.05ms. File-write→first presentation, small samples, not robust p99; state/error/fix passed. Idle CPU0 means zero observed ticks. Emoji loaded/rendered in GL; mapped-file test alone misses copied font data. Cold GL context/shaders ~213ms; targeted font selection already avoids system-wide scanning.
Memory is requested PSS+deduplicated DRM, not exclusive VRAM; shared system allocation may overlap PSS. CPU0 DRM means no app GPU descriptors. See prior comparison for full-screen memory. Basic absolute geometry/input only; no full controls/IME. Dependencies mlua0.12.2/Luau, notify8.2.0, glutin0.32.3 verified permissive by cargo-deny; no system installs.
Validation: fmt/clippy -D warnings,20 tests, CPU-only build, GUI state/error/fix + demo on both compositors, independent cargo-deny graphs and CI38461567225 (8265525) pass. ~2.5h elapsed since first GL edits; 2648 net Rust lines plus fixtures/probes; raw evidence private .research/feature3. [Error screenshot](examples/hot-reload-error.png). Verdict/Gate B: **adjust — retain native GL + small layers/CPU fallback; improve cold startup and off-thread reload baking; proceed to Feature4 only after owner OK**.

## Feature 4 milestone — authoring decision (2026-10-13)

Compared one identical search/sidebar/keyed-grid panel in three formats.
Choose Luau tables/functions: existing parser/runtime, components and imports;
KDL is slightly shorter but adds binding/component conventions; QML-like adds a grammar.
Static Luau checking is not in mlua; strict Rust property/binding validation remains essential.

| Format | UI lines | With shared behavior | Syntax check | License |
| --- | ---: | ---: | --- | --- |
| Luau | 31 | 44 | actual compiler pass | MIT |
| KDL + Luau | 27 | 40 | kdl6.7.1 parse pass | Apache-2.0 + MIT |
| QML-like | 34 | 47 | proposal only, not parsed | own MIT/Apache |

No rendered runtime: memory(PSS+DRM)/CPU/pacing N/A. Syntax spike only, no production
UI code changed; nice10/Cargo-j2 used. No system installs. ADR0002 contains the
side-by-side sources, criteria and replaceable frontend boundary. Runtime layout
and model semantics are next; no implied parity from source counts. Verdict: **on track**.

## Feature 4 milestone — typed frontend, layout and lists (2026-10-13)

Luau now builds the common view tree; Taffy handles rows/columns/grids, padding,
alignment, flex growth and parent percentages. Reusable functions/imports work.
Repeat keys define scoped identity; duplicate keys reject the whole update.
Scroll clipping, source diagnostics and content-surface resizing are implemented.

| Private static toy panel | PSS + DRM MiB | CPU % / interval | Startup ms |
| --- | ---: | --- | ---: |
| niri,800×480 logical,scale1 | 67.81 (43.44+24.36) | 0 /10s | 229.53 |
| Hyprland, same panel,scale1.5 | 97.60 (47.60+50.00) | 0 /10s | 238.85 |

Short smoke measurements, not60s claims; static pacing N/A, and no equivalent
QML port yet. Four model rows, no color emoji: these totals are not comparable
to the old full stress panel. Private/nested only, nice10/Cargo-j2. View fits
smallest output viewport; independent per-output layouts remain a limitation.
25 tests + fmt/clippy/independent cargo-deny pass; imports/cycles/source errors,
key uniqueness/action args/relative widths/grid rows/pixel clipping verified.
Taffy0.15.0 MIT; no system installs. Current example is a toolkit proof, not the
real launcher. Next: port sidebar/app grid and measure equivalent QML. **On track**.

## Feature 4 milestone — real launcher sidebar/grid port (2026-10-14)

Ported NacreAppBrowser's sidebar, responsive app tiles and bottom search field
into reusable Luau components. UI69lines + behavior34 =103; comparison regions
are232QML lines (sidebar107,grid107,search18), not the full1310-line browser.
Primary UI has no hand-listed rows/visibility; legacy JSON moved to legacy-ui.

| Matched63-item static panel / run | PSS + DRM MiB (components) | CPU % /60s | Startup ms |
| --- | ---: | ---: | ---: |
| GL / first | 102.81 (52.90+49.91) | 0 | 273.28 |
| GL / later | 88.79 (38.88+49.91) | 0 | 94.25 |
| QML / first | 193.77 (106.04+87.73) | 0 | 277.05 |
| QML / later | 192.75 (105.02+87.73) | 0 | 154.62 |

PrivateHypr virtual2880×1800/1.5/120Hz; static pacing N/A(one frame), notphysical.
Same dataset/SVG override/loadedfonts, including3rendered color emoji; first private
shader caches fresh, OS/other-agent load uncontrolled. Startupfirst populated
callback/presentation receipt; phase boundary can differ byone refresh. No live run.
SourceNacreAppBrowser public/local identical, SHAa3d574e4; ranges489–595/651–757/
1029–1046. QML regions contain favorites/hidden-item/key handlers not yet all
ported: LOC is scope/count evidence, not equal-behavior compression orfullparity.
SystemXDG catalog uses existingMPL2 desktop parser only in GPL shell; Bead graphs
remain permissive. App execution/ranking/favorites/other-route panels stay later;
current callbacks select/navigate UI state. Static logo/theme and text-fit/elision
still have limitations. Pixel geometry checked in [screenshot](examples/launcher-port.png).
27tests + fmt/clippy/cargo-deny pass. No system installs; ~70min since layout
milestone, mostly parser/style/integration. Animation/input/a11y and100reload
resource proof next. **On track for bounded component proof; full launcher not claimed**.

## Feature 4 milestone — missing Gate B tests (2026-10-14)

100consecutive valid source edits/presentations keep owned resource counts and
file descriptors constant. Keyed reorder/insert/delete + reload preserve focus,
selection, scroll and input while the focused item remains present.
Memory has bounded allocator/driver variation; this is not an exactzero-growth claim.

| 100reload path | PSS + DRM before→after MiB | Drift | FDs | Reload p50/p99 ms |
| --- | ---: | ---: | --- | ---: |
| GL,Hypr virtual1.5/120 | 125.46→126.53 (59.52+65.94→60.47+66.06) | +1.08MiB | 17→17 | 100.54/116.02 |
| CPU,niri virtual1/60 | 30.89→31.29 (DRM0,no GPU fd) | +.40MiB | 13→13 | 139.09/144.22 |

GL eachcycle:8stores,50visible interactive nodes,1watcher/subscription,3cached
bindings,7source snapshots,1retained layer,1GPU image,1surface. No new context.
Driver/allocator pools peak above endpoints (GL135.77MiB); final20reload samples
stable126.47–126.53. CPU final50samples stable31.27–31.29; not linear accumulation.
Tolerance2MiB disclosed; strict memory-equality requirement remains **concern**,
while owned counts/FDs/keyed-state tests pass. No subscribed UI services yet beyond
watcher; later backend/a11y subscriptions must be rechecked after integration.
CPU instrumentation initially omitted cached-layer count (reported0); corrected
method reports actual layer count. Removal of focused item currently leaves a
stored ID tombstone; explicit fallback focus behavior is part of input milestone.
27tests + fmt/clippy pass; no system installs, nested/nice10/j2 only. No live tests.
Next: animation/input/focus/basicIME/AccessKit, thennative backends. **Continue with concern**.

## Feature 4 milestone — animation, input/focus, IME and accessibility (2026-10-14)

Retained root position/opacity animations use easing or critically damped spring,
with animation state outside views. Pointer hover/cursor-shapes and visible focus,
Tab/activation/arrows, grapheme editing/selection/caret/preedit are implemented.
Native keyboard + reload/error/edit/fix proofs pass both nested compositors.

| Matched animated63-item panel /60s | PSS + DRM MiB | CPU % | Draw p50/p99 ms | Callback p50/p99 ms | Synthetic present p50/p99 ms | Startup ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| GL native | 104.69 (60.61+44.08) | 2.38 | .029/.136 | 8.347/9.392 | 8.343/9.327 | 361.87 |
| QML retained | 201.48 (106.51+94.97) | 4.97 | N/A | N/A | 8.330/9.412 | 307.74 |

PrivateHypr virtual2880×1800/1.5/120Hz, refresh0; notphysical scanout. Samefixture/
SVG/font/emoji workload; RootOutCubic position/opacity, no per-frame content bake.
Oneupload/layer per scale. Animationstate survives reload/reversal; spring is the
critical-damping variant, not a general configurable physics engine. Cold slowdown
with platform bridge retained, startup/shader-cache improvements deferred.
Text-input-v3 is bound/advertised on both; preedit/commit/UTF8delete tests pass;
actual IME engine composition notyet exercised. AccessKit0.25.1/Unix0.24 MIT/Apache
bridge registers roles/bounds/focus/actions and activatesprivate status; private
registry hitsdbus-broker/systemd activation issue. Screen-reader end-to-end unverified.
Basic flat accessibility tree/primary-surface bounds only; notfull a11y parity.
GPL shell copy/cut/paste uses installedwl-copy/wl-paste; native privatekeyboard
copy/paste proof passes. These helpers are temporary adapters, not ported Python.
Protocol cursor positions currentlycoarse; bidi text-selection geometry remains
limited.31tests,fmt/clippy,CPU-only build,independent cargo-deny pass; no systeminstalls.
Post-platform100reload: total128.99→129.46MiB (+.48), FD22stable, layer/image1stable;
latency108.20/125.33ms. Late pools stable within2MiB tolerance, not exactequality.
No live tests. **Concern: basicplatform paths exist, integration gaps explicitly open**.
NextFeature5 nativebackends; no wait tonight. Off-threadbake/shadercache deferred.


## Feature 5 — native compositor state/actions (2026-10-14)

The same neutral API drives a live Luau workspace indicator and focused title on
both nested compositors. Direct sockets replace process launches in the adapters;
subscriptions sleep when idle, and unsupported actions return explicit errors.
Reconnect, restart and stable workspace identity pass. Performance is mixed.

| Minimal workspace/title panel | Bead / QML+helpers PSS+DRM MiB | Bead state p50/p99 ms | QML state p50/p99 ms | Idle CPU /60s |
| --- | ---: | ---: | ---: | --- |
| Hyprland virtual2880×1800/1.5 | 66.75 / 60.66 | .178 / 7.312 | 1 / 1 | 0ticks both |
| niri window1278×692/1 | 65.14 / 105.10 | 32.026 / 49.071 | 37 / 39 | 0ticks both |

Twenty alternating focus actions per path; p99 is the worst of this small sample.
Bead uses monotonic action-entry→normalized-state observation; QML uses Date.now
(ms resolution) at actual backend call→activeClient update. Neither is presentation
or scanout latency. Reference uses unchanged Nacre QML backends at bad603e6 with
only temporary path/i18n shims; niri's persistent Python event helper is included.
Same 760×112 indicator/title content, not exact skin/font parity or full-shell cost.
Memory is PSS + deduplicated DRM (shared/system-memory overlap possible); QML
niri earlier93.84MiB versus final105.10 shows pool/cache variation.
Zero CPU ticks over 60s means below~.017% of one core, not zero power.
Native focus/close/workspace moves/switches/float pass both; fullscreen and live
output add/window move/remove pass Hyprland. Recorded IPC replay plus topology
mutations cover niri output add/remove/reindex; physical niri hotplug is unvalidated
in its single-window nested backend. niri output geometry refreshes on topology
and ConfigLoaded events; IPC-only geometry changes lack a dedicated output event.
Connection drops preserve IDs; real compositor exit/relaunch via private stable
socket aliases changes session IDs and rejects stale actions. Different live socket
paths require a new endpoint/environment; no unrelated session is auto-selected.
An initial blank-title proof exposed OnDemand layer focus; --no-keyboard-focus
keeps the indicator from taking keyboard focus. Bound title assertions/screenshots
now pass both ([niri](examples/backend-niri.png), [Hyprland](examples/backend-hyprland.png)).
GPL adapters use niri-ipc 26.4.0/persistent command+event sockets and direct Hyprland
request/event sockets. Nacre's Rust migration (Wave 2) should reuse these modules.
242 neutral-crate / 911 adapter / 443 test / 13 Luau lines; 38 tests, fmt, clippy, CPU-only and
workspace/all permissive framework deny graphs and CI38526465164 pass. No system installs/live runs.
**Verdict: Feature 5 complete with concerns: tail latency, Hypr small-panel memory,
and unvalidated physical niri hotplug. STOP before Feature 6; owner acceptance next.**


## Feature 6 milestone — native service foundation (2026-10-14)

Typed clock/audio/battery/network/Bluetooth/tray snapshots now have a view-independent
service owner. Native PipeWire watches default-sink metadata and Props; zbus watches
D-Bus state/owner changes, with SNI+DBusMenu supplied by the MIT system-tray crate.
The bar UI and final measurements are not yet implemented.

| Check | Result |
| --- | --- |
| Private UPower/NM/BlueZ owner loss + NM/BlueZ return | Pass |
| Subscription drop and callback snapshot reentry | Pass |
| Actual private PipeWire and tray item/menu lifecycle | Pending |
| QML screenshots, CPU/GL bar+panel,200+actions/pacing | Pending |

Missing values stay optional. Sensor calls have1s deadlines; disconnected transports
back off, and bus replacement cancels the old tray executor. Tray action calls use
zbus directly because the library helper can suppress method/timeout failures.
Existing PipeWire1.6.9 headers and Clang suffice locally; no machine installs.
CI adds libpipewire-0.3-dev/libclang-dev. Added zbus5.19.0,pipewire0.10.1 and
system-tray0.8.9 are permissive. Native build helper target-lexicon0.13.5 uses
Apache-2.0 WITH LLVM-exception; a version-scoped cargo-deny allowance records its
[additional permissions](https://github.com/bytecodealliance/target-lexicon/blob/main/LICENSE).
40 default tests +1 private proof,fmt/clippy,both deny policies and CI38531712571 pass.
No GPL allowance enters the framework graph. Private owner fixtures do not prove
real hardware or installed system-daemon restart behavior. **Continue Feature6.**

## Feature6 — reference and native bar milestones (2026-10-14)

The actual Nacre QML header was captured without changing its drawn widgets.
The native Luau bar now receives event-driven desktop/backend snapshots and
renders the same capsule/margin/title/status structure, plus an SNI tray slot.
Private unavailable audio/sensors remain explicit; these captures are not
lifecycle, idle, leak or performance measurements. Those proofs remain pending.

| Proof | niri | Hyprland |
| --- | --- | --- |
| QML header | unchanged widget capture,1278×692/1 | unchanged capture,2880×1800/1.5 |
| Native CPU/GL | both inspected,1278×50 bar | both inspected,2880×75 physical bar |
| Tray fixture | native SNI ARGB icon visible | native SNI ARGB icon visible |
| Clean native exit | surfaces=0 both renderers | surfaces=0 both renderers |
| Measurements/lifecycle | pending | pending |

43default tests plus private UPower/NM/BlueZ proof,fmt/clippy,CPU-only compile,
workspace/independent framework deny passed. A native bundle test checks actual
markup/assets,full1920px layout and stable action IDs. Header has no QML tray;
SNI slot is an extension; DBusMenu choices currently use the inline title band.
Bar logo is a fixed palette-resolved fixture asset; runtime status colors use
existing palette roles. No installs/live desktop tests/reference writes.
Use spikes/measure.py --bar-reference / --bar-ui in the private harness;
Hypr adds --virtual120. Public captures in examples/bar-*.png; ignored evidence
in .research/feature6. Real private PW/tray churn and the full matrix remain.


## Feature6 — service lifecycle and fidelity (2026-10-14)

Real private PipeWire and SNI/DBusMenu operations now pass, including daemon,
item and whole-bus replacement. Churn exposed an upstream menu-task leak;
a small MIT source patch ties those listeners to item ownership.
Native title and tray rendering now preserve actual font/icon selection.

| Private lifecycle proof | Result |
| --- | --- |
| Tray add/update/remove, native activation/menu calls | 100 cycles; pass |
| PipeWire null/default sink, volume/mute, restart | 3 restarts; pass |
| Whole private bus replacement | 3 restarts; pass |
| Late churn FD/tasks/subscriptions | 20 /7 /1 stable; drop→0 |
| Late churn PSS,60s idle CPU | 6.36→6.81MiB;0ticks (<.017% one core) |

Initial unpatched PSS6.74→9.24MiB retained; menu watchers never ended on owner
loss. Patched tasks remain7, PSS growth+.45MiB within existing2MiB tolerance;
this is a bounded100-cycle proof, not a one-hour hardware/leak guarantee.
Private bar hit testing proves click mute, scroll volume, ordinary activation,
menu-only activation, right-click prepare and actual DBusMenu clicked events.
Named/app-directory icons precede valid ARGB pixmap fallback; transparent fixture
and same-path asset updates render correctly. JetBrains Mono NF resolves to
JetBrainsMono Nerd Font; a late title now triggers fresh font selection.
Package glyph/count0 precedes status; the updates helper remains future scope.
45tests,fmt,clippy,CPU-only and workspace/six permissive deny graphs pass.
No installs/live runs/reference edits. Full-size mixed-scene matrix is pending;
early smoke runs expose nested pacing variability and are not final verdicts.
**Step3 complete; continue step4, then stop after Feature6.**


## Feature6 — final full-size matrix (2026-10-14)

Both private outputs now use2880×1800/scale1.5, with50px bars and a distinct
1164×654 GL panel through one shared EGL context. CPU bars have the lowest
measured footprint; native services sleep between changes. Lifecycle/churn passes,
but nested title latency and niri mixed-surface pacing remain concerns.

| Mode | PSS / DRM / total MiB | State p50/p95/p99 ms | Title presentation p50/p95/p99 ms | CPU ticks/60s |
| --- | ---: | ---: | ---: | ---: |
| niri CPU alone | 23.00 / 0.00 / 23.00 | 1.63/13.46/36.17 | 35.15/54.46/82.48 | 2 |
| niri CPU panel | 63.03 / 41.88 / 104.90 | 0.88/25.75/100.19 | 45.23/63.89/152.34 | 2 |
| niri GL alone | 42.76 / 22.27 / 65.03 | 1.08/2.04/2.28 | 33.95/35.62/39.46 | 2 |
| niri GL panel | 54.78 / 54.46 / 109.24 | 1.62/2.39/2.77 | 35.25/50.07/52.42 | 2 |
| hyprland CPU alone | 21.95 / 0.00 / 21.95 | 2.19/3.20/3.36 | 17.90/18.25/18.54 | 1 |
| hyprland CPU panel | 63.64 / 41.89 / 105.53 | 2.15/3.29/3.65 | 17.86/19.21/23.94 | 1 |
| hyprland GL alone | 43.36 / 22.34 / 65.70 | 2.13/3.28/3.59 | 17.79/18.34/21.98 | 1 |
| hyprland GL panel | 55.19 / 56.48 / 111.67 | 2.14/3.40/4.05 | 17.70/19.15/20.46 | 2 |
| niri-qml-bar | 91.00 / 27.00 / 118.01 | N/A | N/A | 0 |
| hyprland-qml-bar | 91.54 / 32.28 / 123.82 | N/A | N/A | 0 |

One60s interval per row,220 focus actions per native row, separate8s pacing below.
State: native action-entry→normalized-state observation. Title: same action-entry
→per-update Wayland presentation feedback (CLOCK_MONOTONIC), not physical scanout.
PSS+deduplicated DRM can overlap system RAM. Services are inside the native process;
test compositor/windows/tray/action driver excluded. QML is unchanged50px NacreHeader
at9e39583 with deterministic transport shims, no tray extension/helper cost; QML
action timings are N/A because shims do not call the compositor. Widget counts and
status availability differ; this is a bar-component comparison, not full-shell savings.
Niri nested60Hz /Hypr virtual120Hz. Private caches start fresh per compositor; later
cases are warm. Diagnostic counters enabled; driver/allocator variability remains.
Source def7255, reader-only fix ab5bf02; failed/superseded rows retained privately.
**Feature6 complete with concerns; keep CPU bar option and shared GL capability.**


## Feature6 — pacing and ownership verdict (2026-10-14)

Unchanged-content drawing is cheap; content preparation and presentation cost more.
Continuous8s callbacks below are separate from idle and focus-action workloads.
Hypr stays near its virtual8.33ms period; niri with a GL panel often needs33ms.
Actual hardware scanout and long-duration memory behavior remain unvalidated.

| Mode | Draw p50/p99 ms | Bar period p50/p99 ms | Panel period p50/p99 ms |
| --- | ---: | ---: | ---: |
| niri CPU alone | 1.990/5.243 | 16.883/48.736 | N/A |
| niri CPU panel | 1.472/1.624 | 32.444/89.428 | 32.461/118.133 |
| niri GL alone | 0.063/0.172 | 16.489/17.756 | N/A |
| niri GL panel | 0.062/0.173 | 33.260/34.604 | 33.256/34.534 |
| hyprland CPU alone | 0.964/1.193 | 8.337/9.098 | N/A |
| hyprland CPU panel | 0.959/1.254 | 8.328/9.106 | 8.329/9.247 |
| hyprland GL alone | 0.031/0.130 | 8.326/9.068 | N/A |
| hyprland GL panel | 0.034/0.134 | 8.343/9.116 | 8.343/9.116 |

Draw excludes GL swap waiting and UI-thread content baking; it is not update latency.
Niri shared-surface pacing misses its16.7ms synthetic period. No optimization applied.
All8native rows pass100rendered tray add/exit cycles: late PSS growth.004–.65MiB;
FD37–43 per mode stable, live bus tasks7/retained watcher1/desktop subscription1,
bar layer1 and CPU0/GL1 images stable. Final service owner100cycles PSS6.48→6.58MiB,
FD20/tasks7/retained1;3PW +3bus restarts, mute/volume/menu calls and routes pass;
zero CPU ticks60s. One tick in a native bar interval≈.017% one core, not zero power.
Initial menu-task leak, completed watcher handles and owned Fontconfig matches were
fixed with small MIT source patches. Rendered-churn allocated heap/Luau diagnostics
separated them; no forced GC, allocator trim or tolerance inflation. Fontconfig
match ownership fix changes100cycle PSS64.07→64.09MiB; initial+2.23MiB failure retained.
GL-unavailable companion exits with a bounded explicit error and releases surfaces.
45tests/fmt/clippy/CPU-only/private sensors/workspace+six framework deny graphs and
CI38551709096/38552860857 pass. Captures inspected; no installs/live/reference edits.
Continuation≈2h; 554net Rust/397Python/4Luau lines, ~4k MIT vendor lines excluded.
**STOP after Feature6. Component kit is roadmap-only before/at GateC.**


## Feature6 — request/event/state tail diagnosis (2026-10-14)

Recording comparisons use220focus actions per path with the same trace boundaries.
Hypr has four snapshot queries per changed state; reply waiting dominates tails.
Recording-on has a larger tail in this run, but the captured delay is in a reply
wait, not its synchronous append. Samples are ordered controls, not a causal trial.

| Probe, recording | State p50/p95/p99 ms | Action-thread CPU p99 ms |
| --- | ---: | ---: |
| niri, off | 29.69/30.94/31.70 | 0.101 |
| niri, on | 29.61/31.27/31.70 | 0.178 |
| hyprland, off | 1.12/1.74/1.89 | 0.200 |
| hyprland, on | 1.05/1.39/9.60 | 0.233 |

Final Hypr worst356.58ms: action reply.255ms; second snapshot query (workspaces)
wait355.91ms with.046ms worker CPU; normalized state356.53ms after action-entry.
Post-reply→recorded interval p99 off/on .023/.086ms (includes trace logging),
event→state p99 wall1.50/1.33ms, CPU.425/.655ms. The p99-defining9.60ms case
has8.57ms action-reply waiting; state is observed.049ms after normalization.
This local Unix-socket trace does not measure processing inside the compositor or
physical presentation. It provides no evidence for blaming append alone.
No-bar niri probe≈29.6ms median with ~.1ms CPU; request waiting and workload
coupling matter. With bars, title p99 ranges18.54–152.34ms across final cases.
Retain these concerns for GateC; shared-surface scheduling/content baking remain
future measured work. Reproduce privately with spikes/measure.py --bar-ui,
BEAD_BAR_MATRIX=1 (Hypr adds --compositor hyprland --virtual120); canonical tool,
Cargo target/raw directories required. Raw evidence .research/feature6/owned-*.

## Feature6b step0a — non-blocking swaps, remaining pacing concern (2026-10-14)

GL now requests swap interval zero; each surface retains its own frame-callback
limiter. This removes the deliberate EGL vsync wait, but does **not** establish a
niri pacing pass. Nested niri still alternates surfaces; Hyprland medians stay at
120Hz with occasional tails. Idle samples vary, so unchanged idle CPU is not yet
proven. No physical test performed; owner approval is required before one.

| Pair/output | Bar presentation p50/p99 ms, before→after | Panel p50/p99 ms, before→after | Idle ticks/60s before→after | After frames/10s bar/panel |
|---|---|---|---|---|
| CPU+GL/niri60 |33.56/35.20→32.49/40.78|33.55/35.02→32.53/49.12|5→11|290/273|
| GL+GL/niri60 |33.30/34.27→32.45/33.51|33.32/34.56→32.46/33.72|5→9|305/305|
| CPU+GL/Hypr120 |8.33/9.39→8.33/9.68|8.33/9.42→8.34/10.33|7→5|1188/1171|
| GL+GL/Hypr120 |8.34/9.35→8.34/9.62|8.34/9.42→8.34/15.68|6→5|1183/1172|

Both private outputs2880×1800/1.5; baseline Feature6 executable preserved.
Presentation receipt is synthetic compositor feedback, not scanout. Frames stay
within output-rate×10s+3 startup grants. One CPU tick/60s=.0167% of one core;
failed-audio retry work is included. After steady GL swap maxima .15–.63ms;
niri activation p99 .053–.056ms in a separate traced diagnostic.
Two CPU surfaces in niri reach16.14ms median, excluding a blanket nested-output
30Hz limit. Callback batching/pre-commit did not improve GL cadence and introduced
16.68ms Mesa waits; reverted. Initial refresh_ns=0 parser failure also retained.
[Glutin](https://docs.rs/glutin/0.32.3/glutin/surface/enum.SwapInterval.html)
recommends Wayland callbacks; [Mesa's source](https://gitlab.freedesktop.org/mesa/mesa/-/blob/main/src/egl/drivers/dri2/platform_wayland.c)
shows a sync-callback throttle remains even with interval zero. This is a mechanism,
not proof that it explains all residual latency. Keep step0a open.
Evidence ignored: .research/feature6b/pacing-before-*,pacing-after-*,pacing-diagnosis-*,
pacing-control-cpu-niri,pacing-batch-check-niri. Reuse measure.py --bar-ui with
BEAD_HARDENING=1/BEAD_BAR_MATRIX=1; BEAD_HARDENING_BINARY selects the preserved
control. --panel-renderer supports CPU/GL diagnostic pairs; --test-passive creates
click-through surfaces with no exclusive zone for a separately authorized test.


## Feature6b step0a — authorized physical acceptance (2026-10-14)

One owner-approved bounded check, then stopped: passive bar/panel, private bus/XDG,
no focus/exclusive zone, no screenshot/title recording or config changes.
The real120Hz output removes the persistent nested33ms cadence. CPU+GL meets
steady presentation pacing; GL+GL still misses≈1.8% of refreshes. This is an
additional acceptance sample, not a matched physical before/after comparison.

| Pair | Bar period p50/p99 ms | Panel period p50/p99 ms | Steady samples bar/panel | Swap maxima bar/panel ms |
|---|---|---|---|---|
| CPU+GL |8.333/8.337|8.333/8.338|960/953|N/A/1.045|
| GL+GL |8.333/16.667|8.333/16.667|943/943|1.323/.221|

2880×1800/1.5/120Hz,10s each; exclude first2s from quantiles. Hardware feedback
kind7, refresh≈8.333313ms; total frames1148/1140 and1168/1167, within the rate cap.
Callback receipt p99≈12.26–16.03ms differs from actual presentation periods.
The logs also show494/511 backend-driven UI commits but only3/9 layer bakes:
most commits repeat layout/script work without changing baked pixels. Preparation
totals1.57/1.79s across10s, supporting the planned script/bake worker investigation.
Do not assume which personal-state field changed; it was not recorded.
Inherited IPC endpoint was stale; initial attempt made no surface. Current session
manager supplied authoritative endpoints. Both exact test PIDs exited; active PID
file removed. No further physical authorization remains. Evidence ignored in
.research/feature6b/physical-niri; step0a remains open for GL tail/idle invariance.


## Feature6b step0b — one Hypr snapshot round trip (2026-10-14)

Hypr snapshots now use one ordered Socket1 batch on the existing event worker.
Four JSON documents are decoded with bounded count; malformed/partial responses
fail normally. All four Hypr state p99 values fall; presentation tails are mixed,
so this does not claim to cure all latency. Niri source is unchanged and its
control rows show substantial run-to-run variation, retained below.

| Case | State p50/p95/p99 ms before→after | Presentation p50/p95/p99 ms before→after |
|---|---|---|
|hyprland-cpu-alone|1.69/3.57/4.47→1.24/2.15/2.39|18.11/24.79/25.97→17.86/23.00/24.09|
|hyprland-cpu-panel|1.50/3.37/4.26→1.50/2.31/2.49|17.82/27.00/389.44→17.73/18.48/18.70|
|hyprland-femtovg-gl-alone|1.81/3.10/3.71→1.33/2.38/2.92|18.03/21.32/22.97→17.84/20.21/27.23|
|hyprland-femtovg-gl-panel|1.80/3.53/4.26→1.29/2.18/2.37|18.56/28.87/34.09→17.49/18.26/19.00|
|niri-cpu-alone|1.34/2.17/2.46→1.60/2.03/2.55|34.70/36.25/36.62→35.55/36.34/49.55|
|niri-cpu-panel|1.27/2.63/2.96→1.30/2.27/3.74|34.47/49.96/52.00→36.68/51.23/53.02|
|niri-femtovg-gl-alone|1.19/2.09/2.51→0.94/5.75/46.40|34.10/35.37/36.03→34.15/147.25/237.69|
|niri-femtovg-gl-panel|1.21/1.91/2.26→0.92/1.58/2.35|34.28/47.36/50.01→34.68/49.87/64.24|

220 native focus actions per row, private2880×1800/1.5 niri60/Hypr120; no trace
recording, preserved interval0 executable before, identical fixtures/30ms gaps.
1s idle checks here are warmup only, not60s idle measurements. UI preparation
still≈12–15ms median; it is separate from compositor request/state/presentation.
Niri no-bar control220actions: action reply28.75/29.96/30.51ms; state28.99/30.37/30.68,
median action-thread CPU.063ms. This is compositor waiting, not UI preparation;
bar activity changes that cost (matrix reply medians≈1ms). Recording-on reply
p99=132.84ms with.104ms median threadCPU; no universal fixed30ms claim.
Hypr before CPU+panel worst519.79ms: reply.30,state1.33,UIprepare23.01ms; much of
that wait follows commit, so its absence after batching alone proves no causal cure.
[Official batch protocol](https://github.com/hyprwm/Hyprland/blob/main/src/ipc/s1/S1.cpp)
orders replies; parser/reconnect/action tests pass. Evidence .research/feature6b/
title-before-*,title-after-*,niri-compositor-cost; future reuse BEAD_NATIVE_BINARY,
BEAD_MATRIX_NO_PACING and BEAD_MATRIX_PROBE_ONLY in the existing matrix harness.

## Feature6b step1 — retained layers and real damage (2026-10-14)

Each view subtree has retained ownership; local text/image/shadow rasters survive
position/clip changes. Fills, caret, selection and progress use direct GL paths;
changed pixels update existing textures. CPU repaint history is separate from
semantic damage. EGL KHR/EXT damage swaps preserve up to4 Wayland rectangles.
CPU improves, but script/layout preparation still blocks and remains step2 work.

| Workload before→after | CPU % one core | Frame processing p50/p99 ms | UI preparation p50/p99 ms |
|---|---|---|---|
| Launcher typing |87.55→29.61|.213/1.466→.232/.817|39.79/69.09→3.70/16.84|
| Visible list scroll |56.48→24.00|.237/88.768→.666/2.489|79.44/310.45→23.73/224.55|
| Moving progress |85.26→6.99|40.356/56.546→.642/1.404|N/A (native primitive)|
| Hover damaged share |1.29% median/p99|Wire rect186×119|48-item launcher|
| Typing/progress damage |2.64%/.30% median|Typing max51.87%|Progress max.58%|

Private2880×1800/1.5/120Hz Hypr output,8s work intervals, same instrumented control
binary with the old monolithic cache. Typing127operations; scroll53 at150ms gaps;
progressnative120Hz requests. Frameprocessing includes prepare/draw/swap; UIprepare
is separate. No-damage frames now commit callbacks without swapping, so sparse
presentation intervals cannot be ranked as continuous frame periods. Progress
presentation8.33/9.21ms p50/p99; strict jitter/typing/scroll tails remain unresolved.
Scroll grid shrink/cull bug repaired, regression tested; matched controls explicitly
use shrink0. Faster scroll queue overwhelmed control shutdown; retained as failure.
Pointer/keyboard readiness failures and a240-item hover50ms Luau-limit failure kept;
no raised budget. Wire proof520damage calls, no INTMAX full-surface substitutions;
only initial1746×981 and actual hover186×119 rectangles. Static GPU uploads occur
once for progress (67local rasters); dynamic fill uploads none. Changed text uses
partial updates. Local-raster bake counts differ from old whole-window counts.
51tests/fmt/clippy/CPU-only pass; private capture inspected. Pixel proofs cover
scroll reuse, same-path update, partial SHM, group opacity and removal/hover damage.
Evidence ignored .research/feature6b/layers-*,scroll-paced-*,hover-bounded-after,
damage-wire-after; invalid/blank-list/timeout runs preserved, not ranked.

## Feature 6b step 2 — script and raster worker (2026-10-14)

Luau and CPU baking now run independently of drawing/animation. GPU resource
acknowledgement precedes candidate validation/store commit. Unchanged reload
pixels reuse rasters; accepted identical generations still request actual
presentation feedback. Strict state/error/recovery proof passes unchanged.
Typing CPU increased slightly; worker isolation is not a total-CPU saving.

| Matched before → after | Result |
|---|---|
| Reload draw-thread preparation median/max | 41.81/234.98 → 0.098/0.153 ms |
| Typing preparation p50/p99 | 3.70/16.84 → 0.091/0.457 ms |
| Typing CPU, one core | 29.61 → 32.66% (regression) |
| Injected 600 ms script-worker pause commits/presentations | 0/1 queued → 71/71 |
| Longest interval around stall | 610.15 → 9.34 ms |
| Whole-run presentation p50/p99 | After 8.37/16.85 ms; tail retained |
| Five reload file→presentation samples | 66.46–75.08 ms (50 ms debounce included) |

Private Hyprland 2880×1800/1.5/virtual120, same workload/binaries as step1;
no physical scanout claim. The injected pause sleeps in script event dispatch;
it does not raise the 50 ms Luau execution budget. Worker initial61 raster builds remain61 across five
filtered-row edits. Prior missing-generation failures preserved; callback-only
no-damage commits were missing feedback, not a renamed marker. One-pixel damage
requests real feedback without re-baking unchanged content. GVfs prints its bus
message after successful teardown; activation during the proof succeeds.
Workspace tests/fmt/clippy/CPU-only pass. Evidence .research/feature6b/takeover-*.
Step0a niri/idle and whole-run jitter remain open. Scroll worker end-to-end tail
is unmeasured; do not substitute draw-thread commit time for script latency.

## Feature 6b step 3 — CPU-first surfaces and idle GL release (2026-10-14)

Automatic bars remain on CPU SHM; companion panels start on CPU, warm native
GL after a callback and return to CPU after two idle seconds. Idle timers then
stop. Tests activate/release twice. Context unbinding alone freed little;
exclusive EGL display termination releases the remaining driver pool.
Warm memory regresses; idle driver release is verified, not allocator trimming.

| PSS + DRM MiB | niri | Hyprland |
|---|---|---|
| CPU bar alone |22.44|21.33|
| Bar + panel GL warm |143.21|150.49|
| Bar + panel after idle release |76.27|75.63|
| Warm DRM → released DRM |66.96 → 0|74.95 → 0|
| Feature6 CPUbar + GLpanel control |104.90|105.53|

Private2880×1800/1.5; niri nested60Hz, Hypr virtual120Hz. CPU-first retains
CPU raster/allocator allocations in addition to warm GL. No claimed full-shell
saving. Final display termination requires exclusive ownership and occurs only
with a CPU primary renderer, no remaining EGL surfaces and a CPU companion.
Explicit GL bars keep their shared transport. Drop-only/unbind-only failures
retained. Reinitialization and final STOP surfaces=0 pass both compositors.
Workspace tests/fmt/clippy/CPU-only pass; policy lifecycle regression tested.
Evidence .research/feature6b/policy-{niri,hypr}-cycle. Step0a still open.

## Feature 6b step 4 — shared kit and edge geometry (2026-10-15)

The permissive framework owns bead.kit; bar and launcher compose it. Named
scale values exactly mirror Nacre size-scale.json, with an explicit drift
check. Existing Orient roles supply color. Nacre motion timings are shared.
Side panels move/anchor from their own edge; popouts open away from bars.

| Proof | Result |
|---|---|
| Tile toggle/detail, slider click/drag/arrow keys | Separate actions / numeric values pass |
| Segmented selection / left and right panel poses | Stable rebuild / inward geometry pass |
| Shared spacing/radii/text/icon scale | Source mirror check passes |
| Private niri and Hypr top bar + left/right kit panel | Six captures presented, inspected, clean stop |
| Blurred artwork / level meter / peek-tooltip composition | Retained image blur / direct fills / composed states |

Kit includes all twelve requested controls, plus Bar/Panel, LevelMeter and
BlurredArtwork; see docs/COMPONENTS.md. Tooltip visibility uses a view hover
binding; slider step count is bounded. No physical screen-reader acceptance.
Bottom bars/panels remain unvalidated. Image blur is not compositor backdrop
blur. Corrected a real worker-startup anchor bug found in the first capture.
Workspace tests/fmt/clippy/CPU-only pass. Private output2880×1800/1.5,
niri nested60Hz/Hypr virtual120. Evidence .research/feature6b/kit-*-final.

## Feature 6b step 5 — errors, durable panels and restart policy (2026-10-15)

Panics log thread/location and worker panics reach the application as fatal
errors. Action and transport failures propagate; poisoned immutable read
snapshots retain their last value with an error log. A source-only user unit
uses Restart=on-failure. Panel state commits atomically after validation.

| Proof | Result |
|---|---|
| Private abrupt kill/restart, both compositors | Panel closed/page restored; no hidden input |
| Panic hook / worker panic | Logged / fatal event forwarded |
| Durable state file | 0600, atomic rename+fsync; transient text excluded |
| Candidate restoration | No store mutation before validated commit |
| Action capabilities | Unknown and ungranted actions fail closed |
| User unit syntax | Pass with private executable substitution; not installed |

State path is XDG_STATE_HOME/bead/panels.json. Bad schema disables persistence
without overwriting the file; only compatible active IDs are restored. Other
views' durable IDs are retained. Closed-target input release fixes a callback
starvation case discovered in the first crash proof. No user-visible prompts.
Workspace tests/fmt/clippy/CPU-only/private service owner checks pass. Source
unit runtime restart is not claimed: the actual proof kills and relaunches
only task-owned clients. Evidence .research/feature6b/restart-* and takeover.

## Feature 6b step 6 — build boundaries and callback hygiene (2026-10-15)

CPU/native GL are default; wgpu/Vello and allocator/thread diagnostics are
opt-in. The old benchmark stays intact outside the default workspace graph.
UI and Wayland modules are split without changing public UI exports. Parked
frame callbacks use generations so reopening cannot grant duplicate frames.
The six implementation steps are done; step0a acceptance remains open.

| Final check / result | Evidence |
|---|---|
| Workspace tests / clippy / feature combinations |60 pass; default/all/CPU lint, CPU build, wgpu-only/all checks|
| Dependency policy |No default wgpu/Vello; workspace + six framework graphs pass|
| CPU bar PSS+DRM, niri/Hypr MiB |23.28 /23.24|
| Bar+panel warm→idle PSS+DRM MiB |niri143.23→76.47; Hypr152.55→77.46; DRM→0|
| Final isolated five reload file→presentation samples |63.91–67.03 ms; 50 ms debounce; state/error/recovery pass|
| Final600ms injected worker pause |67 commits/presentations; longest16.82ms; earlier71/9.34ms retained|

Private2880×1800/1.5, niri60/Hypr virtual120; CLOCK_MONOTONIC feedback verified.
Final pause whole-run presentation8.37/17.00ms p50/p99: strict120Hz remains
unproven. Typing32.66% vs29.61% regression and warm-memory regression persist.
Full scroll worker tail is unmeasured. Parallel final-smoke reloads with longer
tails94–401ms retained separately; isolated repeat is labeled, not substituted.
Source user unit remains uninstalled; no deployment, physical test or Feature7.
Evidence .research/feature6b/epoch-* and takeover/step6-*; failed probes retained.
STOP. Nested niri pacing, idle invariance and physical acceptance remain open.

## Feature 7 — commands, files and bounded helpers (2026-10-15)

A private versioned command socket accepts only the owning UID; its path is
explicit and never discovered from the live session. Typed CLI ingress keeps
audited target/method selectors and return contracts. Future feature handlers
return unavailable errors. Existing helpers stay external Python JSON-lines.

| Proof | Result |
|---|---|
| Audited compatibility catalog |43 targets/120 methods;30 CLI locations/12 shortcut declarations|
| Fixture typed routing / real native CLI |Pass / nested niri and Hypr bar state, errors, cleanup pass|
| Socket bounds |64KiB frames; version/ID correlation;0600 socket;0700 parent; owner credentials|
| Runner/helper |Argv only, finite deadline and stream bounds, own process-group cleanup, helper ID checks|
| Files/watch |Regular-file byte bounds, atomic replacement, coalescing and last-good validation pass|
| Workspace tests / fmt / clippy |66 pass / pass / pass|

One-shot helper invocations close stdin and collect the existing service's
response; no helper code was ported. GUI helper-free idle remains unchanged.
Source catalog availability is explicit; notifications/dashboard/etc. are not
implemented early. Source/shared protocol tests use private fixtures, and GUI
proofs use private buses/outputs only. Evidence .research/feature7/commands-*.
No installed command, live binding, Nacre/Orient write or release.

## Gate C foundation and real sound output — checkpoint (2026-10-15)

One small sound-output panel shares bar-native PipeWire services, with its own
script/raster worker and lifetime. Close-idle drops the worker, VM, raster tree,
SHM pools, renderer and surfaces, then trims free allocator pages. Durable page
state survives separately. CPU selected for this small view; GL remains explicit.

| Private Hypr 2880×1800 / 1.5 / virtual120 | Measured |
|---|---|
| CPU bar / warm output panel, PSS+DRM MiB |20.78 /29–34|
| Ten close-idle deltas from matched bar, MiB |+0.38–3.86; <=5 pass|
| Earlier two-cycle CPU control, MiB |22.83 bar; +2.07/+3.93 closed|
| GL alternative after context/display shutdown |~43 closed vs~23 bar; FAIL|
| Retained GL mappings observed in smaps |Mesa+LLVM ~11.5 MiB, plus anonymous/heap|
| Actions/lifetime |click50%, drag75%, key70%, mute, Escape/outside/button, move/remove pass|

Automatic clipboard transfer passes after seat binding; transport labeling is
corrected by the later explicit core/extension proof. Exclusive
Hypr layers receive outside presses on the panel itself; handle those bounds too
([versioned source](https://github.com/hyprwm/Hyprland/blob/v0.56.2/src/managers/input/InputManager.cpp#L413)).
Facade-only shell imports, non-Nacre example, headless validation, Rust-generated
SDK and18 actual Luau modules/analyzerCI are in place. Numeric-only30s report
implemented; actual duration proof pending. Local68 tests/fmt/clippy/CPU-only/
all-features checks pass. Not a Gate C verdict: niri,60s CPU, matched QML,
reload/state and longer pacing still pending. Evidence .research/feature8/.
Failed probes retained; no unsafe driver-map eviction. Do not infer full-shell
savings from this smaller panel. Typing29.6→32.7% remains optimization work.

## Gate C recovery and diagnostics — checkpoint (2026-10-15)

Real-view reload discovered two integration bugs: panel watches lacked the bar's
50ms debounce; resizing could wait forever on a parked old-size frame callback.
Both are fixed without relaxing generation-specific presentation checks. A
crashed lazy view now restores saved open/page state after the bar is ready.
Command restart recovers only an owner-only refused Unix socket, rechecking its
inode; active listeners, symlinks and regular files are preserved.

| Check | Result |
|---|---|
| Source7117954 CI |38704774853 passed|
| Latest local validation |69 tests, fmt/clippy/CPU-only pass|
| Niri ten close-idle cycles (before recovery fixes) |+0.24–3.93 MiB; <=5 pass|
| Hypr real-view five reload file→presentation |64.92,189.45,65.38,65.11,64.73ms|
| Invalid dependent edit/recovery, closed reopen, SIGKILL relaunch |page preserved; open restored; pass|
| Actual --report duration / CPU total |30.001s /0.299 CPU seconds; includes startup|
| Report privacy/lifetime |numeric/boolean only,0600, closed+current view samples|

Reload includes50ms debounce;189ms tail is retained, not hidden by median.
Early pre-fix failures and build-overlap attempt are not timing baselines.
Evidence .research/feature8/reload-hypr-isolated, diagnostics-hypr,
recovery-checks.log and panel-niri-controls. Final isolated comparable CPU/
pacing/QML and memory acceptance still pending. No physical or live changes.

## Gate C clipboard interoperability and delayed reclamation (2026-10-15)

Correction to early clipboard labeling: both private compositors advertise ext
data-control. Initial binding waited for keyboard capability, not a missing
protocol. Bind on seat discovery too. Automatic ext transfer now works before
keyboard input; forced core mode uses a real keyboard serial/focus. Both native
send/receive UTF-8 and plain-MIME text with independent private wl-clipboard peers;
no clipboard subprocess is used by the product. Unwanted offers are destroyed.

| Private proof | Result |
|---|---|
| Core Hypr / ext niri peer and controls |pass; data never touches live selection|
| Core Hypr closed delta before deferred trim |+0.80,+3.43,+5.13 MiB: FAIL|
| Same three-cycle proof, trim100ms after release |max+2.70 MiB: pass|
| Identical fallback/final464×350 geometry |SHM allocations3 instead of6|
| Source60a537c CI |38705774910 passed|

The delayed trim lets pending retired view packets be dropped before allocator
reclamation. Allocation ownership is a hypothesis; reduced residency is measured.
Final repeated default-mode60s runs and QML comparison still pending. GL remains
explicit; CPU chosen for this400×286 content view. No unsafe map eviction or
raised memory tolerance.69 tests/fmt/clippy/CPU-only pass after clipboard changes;
parallel and one serial wall-clock50ms failures under host CPU/I/O pressure remain
in logs; isolated repeat passed with unchanged limits. Evidence core-hypr,
core-hypr-trim, ext-niri-final, clipboard-*-checks and idle-trim-build.

## Initial-seat clipboard correction (2026-10-15)

Headless Hypr exposes an existing seat with zero input capabilities; neither a
future-seat callback nor a keyboard-capability callback covers that initial
object. Bind existing seats immediately after App construction as well. The
strict pre-keyboard extension transfer now passes on Hypr, as it did on niri.
Source66588ad CI38708160203 passed. Final long measurements continue with
this small correction; QML frame probe is being reused for matched animations.

## Gate C latest visibility intent wins (2026-10-15)

A fast versioned-socket close/open test reproduced a stale worker candidate
closing a newer open request. Controller target now owns dynamic motion/input;
close-hit/focused-action metadata routes real dismissal at input acceptance.
Worker candidates can update content without overriding that target. The same
controller snapshot feeds legacy nacre state (edgeMenu and native panelOpen),
so it reports actual accepted visibility. Header controls are bounded to360px.

| Check | Result |
|---|---|
| Ten rapid reversals before/after |FAIL → pass; real controls/state also pass|
| Latest validation |69 tests, fmt/clippy/CPU-only,18-module Luau analyzer pass|
| Source09b4722 CI |38708841380 passed|
| Prior stable long default PSS+DRM MiB, bar→closed60s |Hypr21.27→25.75; niri22.24→23.77|
| Niri extra closed SwapPss |2.63 MiB; resident+swap delta4.16 MiB|
| Prior Hypr opening/closing p99 |8.76/8.78ms; max9.03ms, virtual120|

Final source lifecycle repeats add keyboard dismissal, rapid intent changes
and post-output/rapid cleanup to the strict memory check. Those plus matched
QML subset timings are pending. Earlier100.32ms niri p99 and failed+5.13MiB
sample remain evidence, not superseded by a favorable run. Sources/evidence:
.research/feature8/rapid-hypr*, reversal-checks.log and final-*-seat.

## Gate C visible content and idle worker ownership (2026-10-15)

Screenshot review caught a blank body despite valid input/feedback. Width was
a false lead: both variants emitted identical draw commands. Full repaint
passed; panel preparation/no-change events unnecessarily drew the old scene.
Draw only changed/geometry/visibility frames; bar updates stay on bar surfaces;
accepted update IDs are per view. Damage remains enabled. Every reopen now
requires actual bright track pixels; marker-only success is insufficient.

| Check | Result |
|---|---|
| Ten pixel checks, niri / Hypr |pass; real controls/rapid/keyboard close pass|
| Latest Hypr post-output/rapid closed delta |+4.38 MiB; <=5 pass|
| Earlier expanded lifecycle |+5.03 MiB: FAIL retained|
| Idle bar baker |drop font/glyph scratch and offscreen entries; retain visible rasters|
| Trim regression test |pixels identical; changed asset lazily rebuilds correctly|
| Local validation |70 tests, fmt/clippy/CPU-only pass|
| QML subset smoke |20 real 200ms slide/fade phases; clock_id=1 verified|
| Source c72fbde CI |38710897960 passed|

Temporary client SHM/static scene dumps removed. Explicit header text widths
fix close overflow (x348 vs x380); earlier row widths alone did not. Current
QML probe avoids idle reporting timers, then tags each animation phase with
actual compositor presentation feedback. No full microphone/device-list claim.
Final 60s paired runs pending; earlier blank-body experiments are not final
pacing/visual evidence. Evidence visual-*, scope-trim-hypr, trim-final-checks,
qml-hypr-smoke. GL floor/CPU tradeoff, typing and inherited 6b concerns remain.

## Gate C persistent idle hint (2026-10-15)

One-shot scratch eviction could be undone by a later clock bake: a prior long
Hypr sample reached+4.997 MiB. Keep the bar baker in idle mode after close;
after any new bake, one250ms quiescence timeout drops scratch, then the actor
blocks again. Opening resumes active caching. No polling timer or allocator
switch. Finished rasters and last-good source snapshots remain available.

| New full Hypr run, MiB | PSS+DRM | SwapPss | Combined |
|---|---|---|---|
| Bar baseline |21.35|0|21.35|
| Warm after60s |22.00|7.30|29.31|
| Closed60s |11.13|4.79|15.92|
| Post-output/rapid/keyboard close |16.05|2.67|18.72|

Ten pixel checks, actions/legacy state, rapid reversal and keyboard close pass;
strict memory predicate includes every cycle, closed60s and final lifecycle.
All three60s CPU rows0.0167% of one core (one tick). Host reclaimed/swapped
pages; resident11.13 is not an11MiB total allocation claim. Even proportional
swap included, closed figures are below the initial bar-only warmup.70 tests,
fmt/clippy/CPU-only pass. Paired niri and60s QML references still pending.
Source364dad1 CI38717169807 passed. Evidence idle-final-hypr and
persistent-idle-final-checks.log. Prior failures/near-limit runs retained.

## Features 7–8 / Gate C measured stop (2026-10-15)

Feature7 transport/catalog/files/helper work and one real default-output panel
are done through GateC. **Recommendation: adjust** pacing acceptance; memory
passes, owner decision pending. Full [report](docs/research/feature8-gate-c.md)
records scope, methods, versions, LOC, failed probes and unmatched boundaries.

| Final metric | Hypr virtual120 | nested niri60 |
|---|---|---|
| Bar PSS+DRM MiB |21.35|22.64|
| Warm PSS+DRM / SwapPss MiB |22.00 /7.30|32.75 /0|
| Closed60s PSS+DRM / SwapPss MiB |11.13 /4.79|26.13 /0|
| Post-output/rapid/keyboard PSS+DRM / SwapPss |16.05 /2.67|24.56 /0|
| Strict <=bar+5 at all checkpoints |pass, even adding swap|pass|
| Idle CPU bar/open/closed %,60s each |.0167/.0167/.0167|.0167/.0333/.0000|
| Native open/close presentation p99 ms |8.56/8.74|41.34/47.27|
| Matched-scope QML PSS+DRM MiB (swap0) |163.09|159.07|
| QML open/close presentation p99 ms |86.11/24.74|43.70/32.99|
| Native command→ready p50/p95 ms |48.63/54.39|74.88/87.65|
| Final five reload file→presentation range ms |56.48–66.97|56.22–60.77|

Host reclaim/swap affects Hypr: resident11.13 is not total allocation. The
small CPU panel's warm resident+swap29.31/32.75 differs from Feature6's larger
GL stress control~105; 6b143–153 retained CPU+GL data together. No full-shell
saving claim. Real controls, all dismissals, two clipboard protocols/peer MIME,
rapid reversal, visible pixels on every reopen, reload/reopen/crash state pass.
70 tests/fmt/clippy/CPU-only pass; sourcefdc99cd CI38719326494 passed. Final
report30.001s/numeric-only0600 passes. Private PW fixture2.14/2.15MiB excluded;
no persistent helper child. Native and QML use actual clock_id=1 feedback.
Keep 0a niri/idle and typing29.6→32.7% concerns; no strict physical120 claim.
Real niri disappearance remains N/A; nested off retained its global. No live
changes, installation, physical/IME/Orca/NVIDIA acceptance, optimization pass
or Feature9. Stop at GateC. Evidence .research/feature8/idle-final-*,
qml-final-*, reload-final-* and report-final-hypr; failures retained.

## Gate C — supervised physical acceptance (2026-10-15)

One immediately confirmed live niri test passed the owner's adjusted p99 gate.
20 cold sound-panel cycles used presentation feedback, private services/state
and a passive IPC-only overlay. Physical timing removes the nested niri tail.
A single 58.33 ms opening gap remains; bounded hover preparation is next.

| Physical 120 Hz, 2880×1800/1.5 | Opening | Closing |
|---|---:|---:|
| Presentation p50/p95/p99 ms |8.333/8.336/16.667|8.333/8.336/8.337|
| Longest ms / sample periods |58.330 /466|8.338 /470|

Refresh8.333313 ms, CLOCK_MONOTONIC; ready p50/p95 45.25/64.64 ms.
Nested niri p99 41.34/47.27 ms was not reproduced. PSS+DRM bar/closed
23.03/25.14 MiB (+2.11), no swap; 2s CPU~5% includes live backend updates,
not comparable to 60s private idle. No screenshot/input injection/audio change,
config/install/release. Both owned product/fixture PIDs exited, PID file removed;
private bus teardown message followed successful feedback. Authorization consumed.
Verdict: Gate C accepted; optimization authorized, stop before Feature9.
Policy: CPU small panels, GL large animated surfaces; prior small GL retained
~20 MiB Mesa/LLVM/heap floor after idle. Keep 0a nested niri/idle invariance
and typing29.61→32.66% open. Evidence ignored: .research/optimization/physical-niri.

## Optimization — bounded hover preparation (2026-10-15)

Audio-icon entry starts the panel's script/bake worker with no Wayland surface.
The selected output's scale is prepared up front; unused work expires in2s.
An unopened hover saves closed state and never maps/focuses a panel.
Initial nested trials show a substantial command→ready benefit; long paired
measurements and final lifecycle validation remain part of this pass.

| Three cycles, 150ms hover lead | Hypr virtual120 | niri nested60 |
|---|---:|---:|
| Command→ready p50/p95 ms |9.99/10.73|7.39/7.72|
| Unused timeout delta from bar MiB |-.49|-.80|
| Closed delta from bar MiB |+2.07|+.10|
| Minimum visible-slider pixels |4674|4674|

GateC cold-ready p50 was48.63/74.88ms; these are exploratory, not a matched
final A/B. All three reopens have pixel checks; actor is closed/no surface
before opening, restored journal remains closed on unused hover. Memory
passes+5MiB in these trials. Source factory receives initial visibility;
prepared scene uses selected output scale, no persistent worker/cache.
Local fmt/clippy/workspace tests pass; a8eb43e CI38723361044 passed.
Allocator hour-soaks (system/arena2/mimalloc3.5.3/jemalloc) now run privately
on both compositors; no adoption before matched timing/idle measurements.
Evidence: .research/optimization/prepared-pixels-{hyprland,niri}.

## Optimization — generation-owned markup bytecode (2026-10-15)

Typing repeatedly compiled unchanged markup, kit and SDK code in fresh VMs.
Compile it once per validated generation; keep evaluations in fresh sandboxes.
The bytecode cache is bounded8MiB, owned by the view and dropped with it.
Counterbalanced A/B/B/A trials show a Hypr CPU benefit without a memory cost;
niri's smaller change remains qualified, not a resolved historical regression.

| Two eight-second trials each | Hypr before→after | niri before→after |
|---|---:|---:|
| CPU % one core, mean |35.26→32.05|36.89→36.26|
| CPU ms per workload operation |22.69→20.45|125.83→124.47|
| Operations per trial |128,122→127,124|24,24→24,23|
| PSS+DRM MiB, mean |116.40→116.38|117.33→117.18|

Same240-row typing workload/binary configuration, no swap. Both cases had
the same background allocator soaks; final quiet validation remains pending.
These are current matched baselines, not direct equivalence to historical
29.61/32.66%. Fresh-evaluation/import-replacement test and all71 workspace
tests pass; fmt/clippy pass. Existing50ms/8MiB Lua limits remain unchanged.
Verdict: retain bytecode reuse; continue the measured typing investigation.
1ade037 CI38724435974 passed. Evidence: .research/optimization/typing-paired-*.

## Optimization — reducing scene copies (2026-10-15)

Tested moving uniquely owned worker packets and cloning only the one primitive
being baked instead of the full scene. Fewer copies did not yield a useful
end-to-end typing improvement. Keep the simpler existing ownership path.
The experimental patch is preserved in ignored evidence and reverted.

| A/B/B/A, two8s samples | Baseline | Copy-reduced |
|---|---:|---:|
| Hypr CPU ms/operation, mean |24.08|24.99|
| Hypr operations per trial |108/126|125/102|
| niri operations per trial |24/61|25/25|

Niri's changing workload rate makes its CPU-percent comparison inconclusive.
No claimed improvement; final quiet trials will add a fixed-rate workload
with every text result acknowledged. Existing maximum-rate proof remains.
Evidence: .research/optimization/copies-paired-* and copies-candidate.patch.
Bytecode source00a2686 CI38725224294 passed. No physical test repeated.

## Optimization — buffers, stacks and cache limits (2026-10-15)

Tested two versus three direct SHM buffers on the actual sound panel.
Two buffers exhausted all reusable slots under nested niri; retain three.
Smaller stacks and a lower raster/texture cap showed no consistent useful
resident-memory benefit in this scene. Keep the defaults and idle release.

| Four cycles, warm PSS+DRM+SwapPss MiB | Hypr | niri |
|---|---:|---:|
| Shared-watcher control |27.59|27.48|
| Two SHM buffers |27.58|FAIL: buffers owned|
| Rust512KiB minimum stacks |27.39|28.52|
| Raster cap16 versus64MiB |27.30|28.64; feedback FAIL|

Completed cycles have visible-track checks; the niri cap run did not finish
the strict closing-feedback proof. Smaller stacks reduce virtual
reservation but are not a demonstrated PSS win; retain normal recursion headroom.
The active scene does not reach the existing64MiB CPU/GL raster/image budgets.
SVGs already rasterize at display size; this matched scene has no JPEG/PNG
decode bottleneck. No new decoder dependency or unrelated image stress claim.
Negative product hooks reverted; patches/binaries retained only in ignored
evidence. CPU/GL idle caches continue to drop through the existing lifecycle.
Evidence: .research/optimization/{shm2,stack512,budget16}-*.

## Optimization — shared scoped file notification (2026-10-15)

Bar and sound view watched the same bundle/palette with separate threads.
Share that notification thread while keeping independent revision counters
and scoped subscriptions. Last owner drops the watcher; registry holds weak
references only. Both surfaces still validate their own source snapshots.

| Four cycles, warm scene | Hypr before→after | niri before→after |
|---|---:|---:|
| PSS+DRM+SwapPss MiB |29.97→27.59|29.85→27.48|
| Threads / FDs |12/46→11/43|12/47→11/44|

All cycles have visible-slider checks. Full controls/native clipboard/dismissal,
rapid intent, close/reopen and Hypr output lifecycle pass both private sessions.
Reload proof exposed an old recovery-wait race: a bar commit plus stale panel
feedback could satisfy generic waits. Commit markers now include surface/error;
proof requires successful panel commit→presentation of the exact update.
Both compositors pass all five reloads, two invalid stages, recovery, reopen
and SIGKILL restoration. Failed old proof retained. Dedicated quiet pacing/
60s idle and final memory acceptance remain pending.
Evidence: .research/optimization/watch-{base,shared,panel,reload-strict}-*.

## Optimization — GL attachments (2026-10-15)

The existing Mesa configuration already has zero MSAA samples. Removing
stencil reduced memory but changed the slider thumb from round to square.
Full-content pixel comparison caught the regression that track-presence alone
missed. Keep stencil; request zero depth alone selected the same driver config.

| Requested config | Actual depth/stencil/samples | Verdict |
|---|---|---|
| Existing |24/8/0|retain|
| No attachments |0/0/0|reject: incorrect slider pixels|
| No depth, stencil retained |24/8/0|identical pixels; no config saving|

Depth-only result is identical on both compositors. Some small-GL cold runs
failed strict closing-feedback counts; surviving runs had large startup gaps.
None is passing small-panel pacing evidence. GL texture-cap16MiB gave no
useful result; incomplete cases are retained, not rerolled into success.
CPU stays policy for small panels; GL for large animated surfaces. Prior
~20MiB Mesa/LLVM residue remains the idle-policy reason. Concurrent-aging
code/driver PSS sharing makes these memory samples unsuitable replacements
for the earlier single-client residue measurement. All experimental GL/SHM/
cache knobs removed from product source. Evidence: .research/optimization/gl-*
and depthless-*; negative patches remain ignored, no image/driver workaround.

## Optimization — release profile (2026-10-15)

Compared identical-source baseline, Thin LTO, one codegen unit and stripping
on the bar+CPU sound panel and fixed-rate launcher typing, both compositors.
One codegen unit saves mapped-code memory; retain it for final validation.
Thin LTO offers no memory benefit; stripping saves disk rather than mappings.

| Three panel cycles /16 typing operations | Baseline | Thin LTO | CGU1 | Stripped |
|---|---:|---:|---:|---:|
| Executable MiB |25.98|25.53|20.02|19.39|
| Hypr bar/warm PSS+DRM MiB |22.02/28.30|22.33/28.55|20.11/26.53|21.59/28.15|
| niri bar/warm PSS+DRM MiB |22.83/30.14|23.36/30.29|21.21/29.01|22.92/30.59|
| Hypr typing CPU %, fixed2ops/s |6.00|6.00|6.37|6.12|
| niri typing CPU %, fixed2ops/s |27.00|23.75|24.62|25.12|

No swap in panel samples. All panel reopens pixel-checked; initial/current
UI feedback must match a non-error commit. CGU1 file-PSS saves1.93/1.58MiB
with essentially unchanged anonymous/shmem pages. One CGU1 Hypr typing
operation missed its500ms injection deadline; do not claim latency success.
These exploratory samples share background aging load; quiet validation follows.
Thin/CGU1 profile rebuilds326.91/178.97s (not matched cold-build timings).
Strip-all loses symbols without a demonstrated mapping benefit; keep symbols.
Panic=abort not benchmarked: it disables the tested worker-panic→Fatal contract;
retain unwinding. No extra combined-profile build without a measured need.
Evidence: .research/optimization/profile-* and profile-builds.json.

## Optimization stress — closed error-scene invariants (2026-10-15)

Concurrent aging/build load triggered existing50ms Lua deadline failures.
The limit stays unchanged; those stress counts will accompany aging results.
A closed failed view cleared primitives but retained invalid layer ranges.
Fix both layer ownership and prepared-frame invalidation for error overlays.

| Regression proof | Before | After |
|---|---|---|
| Closed retained body, with/without prepared frame |invalid layer range|two-item error banner, valid bake|

The test preserves last-good body and stores separately; both paths pass.
No deadline raised or failure marker suppressed. Final quiet controls/reload
proofs remain required. Evidence: optimization-error-overlay-{before,after}.log.

## Optimization — skip unread backend models (2026-10-15)

The recorded physical log had599 title changes on unfocused windows and
603 bar updates; focused title stayed unchanged. Window-list churn was
materializing the bar despite no binding reading that model. Track actual
store reads, including computed inputs; update unread host data without paint.

| A/B/B/A, two20s samples each | Hypr before→after | niri before→after |
|---|---:|---:|
| Native CPU %, background title10Hz |3.70→.45|3.43→.25|
| Bar commits per200 backend updates |201/200→0/1|200/202→0/0|
| PSS+DRM+SwapPss MiB, mean |26.44→26.08|27.56→27.11|

Bar+actual sound panel, fixed separate probe executable, private services.
One extra candidate commit was the legitimate clock. No private title values
printed. All8trials keep genuine focus changes working; an additional strict
proof matches focused-title change to its exact bar presentation on both.
Model state stays current for actions/reload. Used/indirect dependencies still
stage atomically; rejected branches retain prior dependencies. JSON stays
conservative. Dependency cache caps4096names then falls back to ordinary staging.
75workspace tests/fmt/clippy pass. This addresses one idle-noise source, not
all6b0a invariance. Evidence: .research/optimization/model-churn-* and
model-focus-presented-*. Profile/error checkpointcb1ac45 CI38731207553 passed.

## Optimization — allocator interaction checkpoint (2026-10-15)

Arena2 improved early aging on the older baseline, but that gain does not
carry through the selected shared-watcher/CGU1 combination. Test interaction
before adopting another global policy. Keep the ordinary system allocator
pending the one-hour endpoints; no new allocator dependency is introduced.

| Three selected-scene cycles, MiB | System | Arena2 |
|---|---:|---:|
| Hypr warm /closed |26.56/19.73|27.38/20.95|
| niri warm /closed |28.66/20.27|28.25/21.17|

No inherited arena/preload override. All cases passed strict valid-presentation
and pixel checks. Results do not show a useful incremental memory benefit;
no mallopt or environment override added. Long aging remains pending.
Evidence: .research/optimization/arena-interaction-*.

## Optimization — one-hour allocator aging (2026-10-15)

All eight memory-only runs completed3600.70–3606.47s of actual open/release
use,370–372cycles, then60s idle. These are stress-aging measurements, not
functional/pacing passes. Keep the system allocator: mimalloc retains much
more here, while arena tuning adds no benefit to the selected implementation.

| Start bar /hour warm /closed60s, MiB | Hypr | niri |
|---|---:|---:|
| System |7.89/18.16/11.72|8.45/20.29/11.99|
| System arena2 |7.77/16.30/9.96|8.32/17.62/7.21|
| mimalloc3.5.3 |32.53/51.06/51.13|31.09/52.66/50.33|
| jemalloc |44.55/25.67/16.58|16.02/21.65/8.99|

PSS+DRM+SwapPss; same a8eb43e-era executable, shared file pages across eight
clients. Small concurrent PSS is not a single-client~23MiB replacement.
Default allocator integrations via client-only LD_PRELOAD, existing glibc
trimming unchanged; not a fully tuned alternative-allocator purge adapter.
Stress logs include14deadline lines,4layer-range lines and4post-failed-load
state lines (duplicate reporting possible). No budget raised; closed-error
layer/cache bug fixed in current source. Failed early soaks remain separate.
Arena2's older-baseline saving overlaps selected changes; actual selected
scene's closed memory was worse, so no override/dependency added.
All owned aging product/fixture PIDs exited. Strict quiet current-source
acceptance follows independently. Evidence: .research/optimization/aging-*.

## Optimization — final quiet acceptance (2026-10-15)

The final CPU scene passes all20cold/reopen and20prepared cycles per compositor.
Unused hover preparation expires without warm retention; controls and exact
reload/state/crash proofs pass. Keep the memory/latency gains, but typing is
not improved in the final matched trial. Stop before Feature9.

| Quiet selected source | Hypr virtual120 | niri nested60 |
|---|---:|---:|
| Cold baseline→prepared ready p50 ms |70.27→15.67|98.55→13.20|
| Prepared ready p95 ms |17.18|17.89|
| Bar /active warm /closed60s MiB |20.13/26.67/21.32|21.48/29.03/22.05|
| Final lifecycle closed MiB |22.20 (+2.07)|22.25 (+.77)|
| Prepared opening/closing p99 ms |9.04/8.81|34.44/33.95|
| Typing CPU before→after % |59.85→59.54|56.60→57.17|

PSS+DRM+SwapPss, selected samples swap0; warm active median, not reclaimedidle.
Every cycle/closed/final≤matchedbar+5MiB. Unusedhover60s deltas−.75/−.92MiB.
Correction: preparation commits before open. First-presentation proof now
matches exact earlier validated update; wrong/future/error commits fail.
Old late-update timing retained; corrected20-cycle runs save both timestamps.
Niri median phase sensitivity and6b0a pacing/idle invariance stay open.
Historical29.61→32.66% typing remains unresolved; final CPUms/op38.82→38.89
Hypr/180.00→183.56niri. No net typing improvement claimed. Source d72a55f
CI38733902941 passed;75tests/fmt/clippy,default+CPU-only,18moduleanalyzer pass.
Reloadfile→presentp50/p9565.30/71.64 and62.64/71.70ms includes50msdebounce.
Report30.00174s, numeric0600, closed-viewhistory; native controls/clipboard/
dismissal,rapidintent,Hyproutputlifecycle,recovery/SIGKILL pass. Physical
acceptance remains the sole supervised earlier run; no optimized physical test.
Evidence: .research/optimization/quiet-*; full readable report in docs/research.

## Features 9–10 typing/private-notification foundation (2026-10-15, active)

A320-app proof now measures each edit against exact presentation feedback.
Luau construction and repeated font selection dominated the measured CPU cost.
Windowing/native projection/text-only layout reuse show a real CPU reduction;
matched repeats and the full launcher are still pending. No Gate D verdict yet.

| Development trial | Hypr presented p99 ms | CPU median / mean ms per key |
| --- | ---: | ---: |
| Initial traced grid40 keys | 55.49 | 41.35 / not paired |
| Text-patch grid80 keys, numeric-only | 17.29 | 7.32 /7.32 |
| Initial nested niri60 grid | 49.59 | 42.76 / not paired |

Different tracing/counts and contended trials prevent a final before/after claim.
Latest CPU p99 11.12;120Hz niri acceptance unverified, deadline unchanged.
Private protocol/ownership/history restart/four exact OSD presentations and
CPU/no-DRM proofs pass both. Empty/warm MiB15.60/27.49 and16.17/29.91; bus
adds0.66/0.69. Two-second idle is only smoke. Visual/hover/commands/QML pending.
Foundation checkpoint ~1.3 h elapsed, ~2.4k added lines including proofs/docs.
A missing PropertiesChanged signal caused stale inhibition state and was fixed.
No live notification bus, physical desktop or reference repo was changed.
See docs/research/feature9-gate-d.md; ignored evidence .research/gate-d.

## Foundation pause checkpoint (2026-10-15)

Features9–10 pause for the approved daemon foundation; typing is not accepted.
Private notification interaction and crash cleanup pass on both compositors.
A child inherited the UI's blocked SIGTERM mask; child-only reset fixes cleanup.
No new typing, catalog, ranking or provider work was started after the redirect.

| CPU-only nested smoke | Hypr120 | niri60 |
| --- | ---: | ---: |
| Empty / warm / closed PSS MiB |16.01 /26.16 /22.26|16.58 /26.92 /23.23|
| DRM |none|none|
| Protocol / hover-replace / action / restart / cleanup |pass|pass|

81 tests, fmt, clippy warnings denied,22-module Luau analysis pass.
Idle only2seconds; QML comparison and GateD acceptance remain pending.
Checkpoint ~540 added lines plus docs; prior typing evidence remains unchanged.
Next: F0 ADR and F5a–F5e; STOP before resuming Features9–10.

## Foundation F5a — hostable services (2026-10-15)

Notification ownership is now supplied by its host, independent of a UI process.
The core uses the host's current-thread Tokio runtime and injected bus/history.
Existing private shell operation remains a wrapper; no live bus was touched.
The same protocol suite passes headless and through both nested shells.

| Regression smoke, CPU retained | Hypr120 | niri60 |
| --- | ---: | ---: |
| Bar / first warm / closed5s PSS+DRM MiB |20.49/26.70/21.84|22.32/28.34/22.62|
| Final lifecycle delta from bar MiB |+2.11|+.30|
| Three closes + lifecycle within bar+5 |pass|pass|
| Notification protocol / real action / hover / restart / bus cleanup |pass|pass|

No DRM allocation. Short5s samples, concurrent nested tests elsewhere; no new
CPU/pacing acceptance or matched QML claim. Accepted GateC baseline unchanged.
83 tests/fmt/clippy pass. New API fixture covers competing/same-connection
starts, errors, history/DND restore, slow subscriber, stop/drop and reuse.
64-call queue,3s deadline, latest-one watch snapshot; no unbounded update queue.
CI cargo-tree default/all-features guard excludes store/UI/render/Wayland deps.
Desktop composite adds config/call/stop while default bar behavior is unchanged.
Individual audio/tray task hosting remains transitional for later daemon adoption.
One initial full run hit the existing stale-socket fixture; focused and full
reruns passed without changing its proof. Evidence .research/foundation/f5a-*.
~470 added lines + fixtures/docs, ~30min including builds and nested checks.

## Foundation F5b — deterministic headless harness (2026-10-15)

bead-test exercises real Luau, stores, Taffy, AccessKit and CPU retained drawing.
Bundled permissive fonts and explicit icon fixtures remove host discovery.
Goldens are reviewed regression fixtures, not a claim of completed QML parity.
The paused OSD fixture shows text outside its pill at scale1; recorded gap.

| Headless check | Result |
| --- | ---: |
| Transform/opacity frames after initial composite |120|
| Actual Taffy passes / raster builds during those frames |0 /0|
| Initial warm rasters including opacity composite |6|
| Static symbol fixture versus full pinned font, differing pixels |0|
| Workspace tests |88 pass,1 opt-in ignored|

Pointer/text/Tab/editing scripts and AccessKit focus/value/actions are checked.
Goldens cover controls, populated notifications and OSD; empty main/bar checks
also render through the harness. Real content changes increment raster count.
Replay rejects CPU/p99 budget misses and reversed/empty timestamps; synthetic
replay is not a measured typing acceptance. Failure writes actual/diff PNGs to
CI artifacts. Tolerance2 channel levels/.1% changed pixels, never auto-updated.
Fmt/clippy-Dwarnings/CPU-only check/independent cargo-deny pass. No compositor.
The reused variable symbol fixture was wrong; full font isolated it. A static
subset matches exactly. Capture used isolated fonttools4.60.1 (not runtime/CI).
~700 authored code/test lines plus licenses/assets; ~20min including capture.
No production font or Feature9 UI changed; no new QML/runtime benchmark needed.

## Foundation F5c — crash safety (2026-10-15)

The UI loop now aborts a stall, with fixed-code reports and core dumps disabled.
Only a successfully acknowledged frame promotes its validated UI bundle.
Startup can use that bundle while original source files are repaired and watched.
Three unclean starts in120seconds select a trusted CPU bar without services.

| Private nested proof | Hypr120 | niri60 |
| --- | ---: | ---: |
| Actual UI-stall start→watchdog abort ms |5077.76|5071.18|
| First-frame / broken source+palette boot / fixed reload / safe mode |pass|pass|
| Bar / first warm / closed5s PSS+DRM MiB |21.67/27.11/22.30|23.19/30.50/23.18|
| Final lifecycle delta from bar MiB |+1.47|−.005|

Three closes and lifecycle within bar+5; no DRM. Short5s idle with other nested
QA/builds active; no new pacing/idle invariance or matched QML acceptance.
94 tests/fmt/clippy/CPU-only check pass; one opt-in service test and one explicit
watchdog-child entry ignored normally (the parent executes the child).
Headless tests cover real abort, clean heartbeat stop, exclusive crash counter,
path escape, safe-bar PNG, first-frame gating, source/palette recovery, original
reload, pending budget and out-of-order feedback versus newer generations.
UI heartbeat dispatch1s; watchdog6s includes one dispatch interval of grace so
an actual busy stall exceeds5s. Persistence uses wp_presentation when present;
otherwise exact-generation frame callback acknowledgement, never timing metrics.
Pending source snapshots8MiB/32 generations, raw bundle8MiB/encoded32MiB;
cap failures retain the old UI. Relative files only; no view/store payload saved.
Crash history atomic/locked; dead restore directories reaped, live owners kept.
Unit already Restart=on-failure; repo LimitCORE=0 added, nothing installed.
~1100 added code/test/proof lines; artifacts .research/foundation/f5c-*.

## Foundation F5d — Sources (2026-10-15)

Views can project the same Source data into stable stores, independent of transport.
Snapshots and RFC7396 patches retain the last good picture on gaps/disconnection.
The local notification view uses this path; fake and host APIs pass headless.
Protocol v1 was unpublished at entry, so the permitted fake fallback is used.

| Private nested Source bridge smoke | Hypr120 | niri60 |
| --- | ---: | ---: |
| Notice protocol / buttons / hover / restart / bus cleanup |pass|pass|
| Bar / first warm / closed5s PSS+DRM MiB |21.09/27.14/22.00|22.86/30.16/22.83|
| Final lifecycle delta from bar MiB |+1.77|−.04|

Three closes/lifecycle pass bar+5; no DRM fds.5s idle/contended, no new QML,
CPU or pacing acceptance. Native smoke sourcee3134fd; final origin-order fix
is additive metadata/guard logic with a deterministic out-of-order test.
103 tests/fmt/clippy pass; CPU-only check/independent core cargo-deny pass.
Headless source tests cover coalesced base, gap/resync, new boot/old reader,
atomic rejected projections, retained UI +2s reconnect mark,50ms→2s backoff,
call deadlines/no mutation replay and identical Luau bindings.8MiB/64depth/
65536-value payload cap,64 scoped listeners; disconnect shares Arc data.
Injected handle Source runs the same private-bus wire suite without a UI.
Stop closes admission and flushes accepted state; failed start cannot write.
Repeated stale-socket fixture failure reproduced with inherited fork descriptors:
strict active-socket refusal kept; fixture requires confirmed ECONNREFUSED.
No pinned adapter/upstream transcript replay yet; no GPL dependency in framework.
~1700 added lines including tests/docs; no catalog/ranking/providers or new typing.
Wire authentication/reconnect and daemon30-day history/chaos acceptance remain
later migration/F6/F7 work. Current history is bounded256 records/3MiB.

## Foundation F5e — background protocol and audit (2026-10-15, STOP)

Standard compositor blur is opt-in and requires an advertised global +Blur flag.
Both private builds advertised it; no vendor shader/rule or dependency was added.
Lifecycle checks pass; this does not accept material/rounded/visual blur parity.
The requested font/cache/animation audit reports gaps without rebuilding caches.

| Nested bar+sound proof | Hypr120 | niri60 |
| --- | ---: | ---: |
| Standard Blur advertised /objects created /destroyed |yes /7 /7|yes /4 /4|
| Bar /first warm /closed5s PSS+DRM MiB |21.51/27.51/21.80|22.74/28.05/22.78|
| Final lifecycle delta from bar MiB |−.84|+.04|
| Selected active mapped-font PSS KiB |1196|1196|

All closes/lifecycle<=bar+5; no DRM client fds. Short contended5s samples;
no new QML/pacing/CPU acceptance. Hyprland's tested build advertised the standard
contrary to plan assumptions; defaults off, owner layer rules unchanged.
106tests/fmt/clippy/CPU-only build/deny checks. Guard tests: opt-out/no global/
no Blur/unknown flags/zero-invalid sizes. Exact object lifetimes tested nested.
120 combined fade+transform frames:0 Taffy/0 content builds after one composite.
Opaque transform-only:0 layout/content builds but120 direct composites with
procedural fills; universal no-raster invariant not established. Fonts mapped
with shared file data. Pixel/GL/opacity64MiB; bytecode/Source/recovery8MiB caps.
Gaps: glyph/font internals, legacy raw texture map, selector/materialized caches,
metadata and optional per-scale GPU accounting. Checked, not rebuilt.
CPU small panels/GL large animated views; earlier ~20MiB Mesa residue unchanged.
~200 protocol/proof/audit lines plus report. Raw .research/foundation/f5e-*.
STOP after F5e. Adapter deferred under allowed unpublished-v1 fallback; Features
9–10/typing remain paused until owner OK and migration phase. See F5 report.

## B0073 — Status controls and parity (2026-10-24)

Wi-Fi and audio are visibly closer to the QML reference; controls reuse guarded
published calls. The updates consumer is ready but its private daemon count is
unavailable. Dashboard reveal still starts later despite faster dispatch acks.
Full Arch CI and private capture gates passed; no live desktop changes.

| Nested CPU, scale 1.5 | Before | After |
| --- | ---: | ---: |
| Wi-Fi whole-image RMSE % | 10.248 | 6.195 |
| Audio whole-image RMSE % | 7.200 | 5.482 |
| Median idle PSS+DRM+SwapPss delta MiB, three pairs | reference | +0.027 |

Remote relative memory gate passed; no populated-panel/laptop acceptance claimed.
All residual differences and source/timing gaps are in B0073 with paired evidence.

## B0113 phase 1 — widget model and bar host (2026-10-25)

The existing bar now lives in validated Luau packages on a general strip host; default drawing expressions and IDs are preserved.
Four regenerated bar goldens and all four native control/candidate bar states are pixel-identical; private nested CPU QML pairs meet the raw RMSE gate.
Five alternating pairs pass the +0.5 MiB median unreclaimable gate; all deltas are anonymous, SHM is identical and DRM/SwapPss are zero.
Verdict: phase 1 passes on the B0118-integrated source; Nacre opaque store registration is requested and the laptop reference cap remains with the lead.

| Metric | Control | Widgets | Gate |
| --- | ---: | ---: | --- |
| Header RMSE %, idle/hover/active | 5.726/5.713/5.898 | 5.726/5.713/5.898 | no worse |
| First-key UI CPU median/max ms | 3.827/3.865 | 3.819/3.857 | ≤8 |
| Nested first presentation median/max ms | 4.336/4.400 | 4.119/4.386 | reported |
| Idle paired median Δ unreclaimable MiB | — | +0.133 | ≤+0.5 |
| Launcher paired median Δ MiB | — | +0.113 | ≤+0.5 |
| Typed paired median Δ MiB | — | −0.492 | ≤+0.5 |
| Early closed paired median Δ MiB | — | +0.121 | ≤+0.5 |
| Idle/closed file PSS MiB (info) | 20.261/20.914 | 20.472/21.060 | not gated |

Full remote CI and isolated bar checks pass on `a9c2f75`; no new system build dependency (`notify` was already transitive).
All outliers remain in the medians; negative typed residency is not an optimization claim. Parity alone fixes the clock display input; timers and measured memory/key snapshots stay normal.
[Sources, component accounting, timing limits, excluded attempts and paired screenshots](docs/specs/0113-widget-system.md).

## B0126 — instant slider drag (2026-10-25)

A visible synchronized child thumb avoids panel raster for pointer poses.
It saves latency in the private nested comparison, but misses both target gates.
Input delivery and nested presentation consume most of the remaining time.
The prototype is rejected; no runtime change is integrated.

| CPU p50/p95 ms | Control | Candidate | QML | Gate |
| --- | ---: | ---: | ---: | ---: |
| Synthetic 60 Hz input→feedback | 65.76/72.86 | 50.23/56.77 | 65.04/79.25 | p95 ≤21, fail |
| Synthetic 120 Hz input→feedback | 37.57/43.62 | 32.02/35.87 | 33.35/50.91 | p95 ≤12, fail |
| Pinned GL 60 Hz input→feedback | 65.73/70.80 | 49.98/54.83 | 65.04/79.25 | p95 ≤21, fail |
| Pinned GL 120 Hz input→feedback | 38.71/46.83 | 32.90/37.18 | 33.35/50.91 | p95 ≤12, fail |
| Fixture first-key UI CPU | 6.981 | 7.415 | — | ≤8 sample passes |
| CPU maximum median added unreclaimable MiB | — | +0.711 | — | ≤1 budget |
| Pinned GL maximum median added MiB | — | +0.914 | — | ≤1 budget |

Pixel-error superiority and general dragging are unproven; no physical-output,
Intel, laptop memory-cap or complete smoothness acceptance is claimed.
[GL, five-pair memory, stage traces and rejected patch](docs/specs/0126-instant-drag.md).

Owner continuation 2026-10-25: accept the measured B0126 thumb candidate and enable
it by default. Continue dispatch, coherent fill/handle and local slider damage,
in that order; revised 120 Hz total p95 ≤25 ms gate. Old rejection is historical.

## B0131 — warm pearl release (2026-10-26)

Prepare the native target before drag, then commit its final pose under cover.
CPU reuses and preimports SHM; GL shares its context and full-size backbuffer.
The cold CPU tail was reveal geometry/composition; build-profile effects are explicit.
Active memory buys smoothness; the declared always-on UI budget is 1 MiB.

| Private niri release build | CPU | GL |
| --- | ---: | ---: |
| Presentation-only20 handoff p95 / render p99 ms | 31.120 / 9.217 | 31.390 / 2.947 |
| Separate image20 blanks / reversals | 0 / 0 | 0 / 0 |
| Capture-loaded handoff p95 ms (≤33) | 36.680, fail | 34.830, fail |
| Matched5 captured median control→candidate ms | 504.506→36.043 | 504.530→33.432 |
| Idle / closed median delta MiB (five pairs) | −0.266 / −0.078 | −0.242 / +0.047 |
| Docked / detached median delta MiB | +4.736 / +8.207 | +21.209 / −13.182 |
| QML / Bead return2px settle ms | 182.964 / 135.557 | — |

Unreclaimable is anon+SHM+DRM+SwapPss; clean-file PSS is separate. Private niri CPU
closed +41.223 MiB (mostly DRM) is frame-history-sensitive in an extra-capture/UI-exit
probe; no combined desktop-memory budget pass. No physical 120 Hz/17 ms claim;
the child backend is 60 Hz and timestamps are software. Host easing is 180 ms OutCubic.
[Methods, matched controls, all cohorts and limits](docs/specs/0131-pearl-release.md).
