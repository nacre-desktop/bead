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
40 default tests +1 private proof,fmt/clippy and both deny policies pass.
No GPL allowance enters the framework graph. Private owner fixtures do not prove
real hardware or installed system-daemon restart behavior. **Continue Feature6.**
