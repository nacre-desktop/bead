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
