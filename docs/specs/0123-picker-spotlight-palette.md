# B0123 — Spotlight overlay, first content and synchronized palette

Status: review candidate; remote validation complete, with the retained GPU memory trade below.

Spotlight floats a centered picker card over the live desktop, using the existing
palette, typography and controls. Its root is a translucent dimmer; it no longer
paints a selected-wallpaper gallery backdrop. Optional compositor blur is not needed.
Layout, dimensions, spacing and motion remain in Luau and theme tokens.

## First content and retention

A catalogue-loading scene is not a content frame. The worker commits its model
privately so hydration can advance, but publishes pixels only after the visible
thumbnail requests settle (or an explicit service error). The panel no longer
falls through to a blank fallback after 750 ms. Worker failures still publish
an error scene. The single resident picker stays prepared after closing, beyond
the ordinary 120-second panel expiry; opening another panel replaces it normally.
This intentionally spends warm memory under SMOOTHNESS FIRST. Never-opened state
is unchanged; there is no new always-on catalogue or image cache.
Budget: ≤1 MiB additional never-opened unreclaimable memory. Warm retention is
explicitly a smoothness trade rather than an always-on feature allocation;
its measured retained cost is reported separately (allowed by SMOOTHNESS FIRST).
Native GL panels prepare their context, surface, image layers and opacity group
in an unpresented backbuffer before the first fade starts. They skip the old
CPU-first frame and its later CPU→GL conversion stall. The first-ever open still
waits for catalogue hydration and GPU preparation; it never publishes a loading
placeholder. Unavailable runtime GL falls back once to CPU for that panel slot;
a replacement slot may try again. Explicit renderer failures remain errors.

General framework primitives: `PanelSpec.overlay` and `retain_when_closed`.
`Session::resolve` reads staged/computed host policy; the shell host's `view.frame` boolean lets Luau opt out of joined frame artwork.
The latter retains only the existing single panel slot, not a pool of hidden VMs.

## Palette

B0109 deliberately showed an optimistic wallpaper with the previous palette.
Selection now remains a generation/identity fence only: new pixels come from the
confirmed image/palette pair, through the existing prepare/accept/commit fence.
Bar and cached/open companion prepare palette changes concurrently behind the
existing output readiness fence; their previous buffers remain visible until
all participants accept. A removed companion settles its old epoch, stale
worker epochs cannot finish its replacement, and failed preparations fence the
rollback too. A different palette snaps the wallpaper in the same coordinated turn; same-palette
changes retain their crossfade, attaching the first target blend before acknowledgment.
Busy SHM buffers retain the final snap in the existing retry slot until an attach
succeeds or a newer selection supersedes it. The surface's palette/revision track
attached pixels, so a superseding retry cannot accidentally fade the old image
under the new palette.
Motion waits until every output using that source commits its poster/palette pair. Nacre warms
its existing settings/file-identity-keyed palette markers for explicitly hydrated
catalogue ids, using bounded work and bounded marker retention; see paired commit.
Never-seen files retain the async extraction path and the old scene until ready.
The daemon's atomic presentation/media observer also waited 500 ms. It now
publishes completed record changes immediately, coalescing each inotify read;
the independent legacy catalogue/engine debounce stays at 500 ms. Real-daemon
tests cover immediate active state, unchanged legacy delay and stale-record recovery.
There is one prewarm helper at a time, at most 128 queued ids/32 active ids, and
64 retained palette markers after each batch (temporarily up to 96 while a batch completes). The existing Orient disk cache is reused; its pre-existing
disk retention is not newly bounded by this change. No resident raster cache is
added to the daemon.

## Acceptance evidence

Full remote Arch CI passes at `3e018ef7` (B0133 integration), `263c408f` (B0125 integration)
and `cbe60901` (fallback proof source), including fmt, clippy with warnings
denied, workspace/trace tests, renderer feature configurations, pinned Luau
analysis, Python regression/packaging suites, private services/tray lifecycle,
the full motion-wallpaper smoke test, and all independent dependency graphs.
The four static/dynamic Spotlight goldens at scales 1 and 1.5 were regenerated
and visually inspected. [Commands and binary pins](evidence/0123/ci.json).

Private comparisons use generated wallpapers, identical palette/settings and
2880×1800 at scale 1.5 in nested niri. Final pairs use Bead’s CPU bar and native
GL companion; the earlier fixed-CPU cohort is reported separately. QML uses its
normal Qt backend. A real private test window sits behind the picker. The
guarded driver checks containment, output geometry, process SwapPss, host
swap-in and pre-arm swap below 64 MiB. Each capture command takes the shared
desktop lock. Screenshot spawn clocks, client buffer submissions and nested
presentation feedback are reported separately; none measures physical scanout.

Paired screenshots: [QML idle](evidence/0123/qml-idle.png) / [Bead idle](evidence/0123/bead-idle.png),
[QML open](evidence/0123/qml-cold.png) / [Bead open](evidence/0123/bead-cold.png),
[QML hover](evidence/0123/qml-hover.png) / [Bead hover](evidence/0123/bead-hover.png),
[QML pressed](evidence/0123/qml-pressed.png) / [Bead pressed](evidence/0123/bead-pressed.png).
The center poster has no additional hover/pressed decoration in either captured implementation.
Bead’s pre-existing + Add control is disabled; QML has an operational add flow.

Intentional QML difference: QML retains its fullscreen selected-image gallery;
the requested Bead Spotlight is a centered card and live-desktop dimmer.
Its toolbar/search/navigation use the existing palette roles, controls and fonts.
The stationary card fades over 300 ms with OutCubic easing; carousel navigation
retains QML’s 300 ms OutCubic travel. Blur and hover-to-change the desktop are
omitted. Font/thumbnail sampling differs. The outer bar’s Updates count reads
150 in QML and 15 in Bead in the final pair; bar count parity is outside this
picker unit. These images are not a pixel-equality claim.

Final normal-GL capture source is `160f8095`; its later fallback-only correction
changes no working-GL branch. [Binary/source pins](evidence/0123/provenance.json).
No live desktop, installation, system configuration, or permissions changes.

## Five-pair CPU memory and first content

Five alternating control/candidate pairs use the same release flags and private
host/output. Unreclaimable is `Pss_Anon + Pss_Shmem + DRM + SwapPss`; the latter
two were reported, and zero, in every sample. Each delta is the median of the
five within-pair differences, not the difference between the two column medians.
Close is sampled after 0.6 s; expired-closed follows a 125 s wait.

| Phase, MiB | Control median | Candidate median | Paired median Δ |
| --- | ---: | ---: | ---: |
| Never opened | 38.090 | 39.266 | +0.082 |
| Closed 125 s | 40.652 | 77.662 | +36.881 |
| Reopened after 125 s | 115.459 | 114.359 | −1.072 |
| Closed again + 0.6 s | 104.434 | 81.990 | −22.455 |

Never-opened growth passes the declared ≤1 MiB budget and the +0.5 MiB regression
guard. All samples remain: idle deltas are +1.176, +0.051, +0.082, −2.789 and
+4.602 MiB. The two larger positive outliers are not a majority; the control
also has an arena-sized increase. The retained 125 s cost is repeatable:
+27.027 MiB anonymous and +9.854 MiB SHM paired medians. This is the explicit
SMOOTHNESS FIRST trade; it is not presented as a memory optimization.
Clean-file PSS is informational: idle medians 19.441→19.237 MiB, expired
18.552→19.451, open 19.337→19.451 and closed 19.098→19.624.

All five controls publish their first companion commit with zero image-cache
bytes; all five candidates publish with 171,439 bytes. After 125 s, control
median first empty buffer renders at 37.011 ms, while the first positive-image
cache proxy renders at 274.065 ms. Candidate first retained content renders at
10.244 ms; its refresh commits at 13.222 ms. The positive-cache proxy can
precede complete thumbnail settlement; the candidate additionally gates on all
visible image handles. Warm presentation feedback deduplicates update ids, so
the refresh presentation median (58.405 ms) is not assigned to the first cached
draw. [All samples and components](evidence/0123/memory.json),
[raw phase and first-commit records](evidence/0123/memory-raw.json).

## Palette presentation and CPU fallback pacing

The old CPU control’s screenshot L1 progress crossed for the image at 125.8 ms and
bar/picker at 1122.8 ms (997.0 ms separation; screencopy scheduling proxies).
With final Nacre `a1e3f865`, the active topic arrives in 239.140 ms versus
740.793 ms before: removing the native observer debounce saves 501.652 ms.
A clean 120 Hz nested run presents wallpaper and bar together at 486.210 ms,
then the picker at 494.346 ms: **8.136 ms separation**. In the matched screenshot
run all three presentation callbacks have exactly the same timestamp;
that run takes 774.770 ms with overlapping screencopy. Feedback reports
`refresh_ns=0`; nominal 8.333 ms scheduling is not physical 120 Hz scanout.
Prewarmed activation still includes the helper's existing state/prepare pipeline;
“instant” here means synchronized presentation, not zero selection latency.
[Before screenshot proxy](evidence/0123/palette-control-analysis.json),
[matched final pair](evidence/0123/palette-integrated-analysis.json),
[clean CPU clocks](evidence/0123/pacing-integrated-analysis.json).

Clean CPU first image-cache-positive commit/render/present improves from
254.130/294.358/296.086 ms to 126.304/127.997/129.585 ms on first open.
Warm and 125-second cached first frames render at 11.299 and 10.531 ms;
cached presentation callbacks cannot be uniquely correlated to that draw.
The CPU fallback still has render p99 37.897/41.785/43.555 ms for cold/warm/expired,
with callback interval p99 48/43/44 ms. Rounded CPU painting dominates these
stalls (27–33 ms; largest primitive 16–18 ms), not scheduler sleep. No zero-drop
claim is made for the CPU fallback. This motivated using the existing native GL
renderer for the wallpaper panel when compiled in, retaining its prepared
renderer past idle rather than promoting and retiring it repeatedly. Explicit
CPU selection and builds without native GL retain the CPU path.

## Final default renderer timing

The final clean Carousel-layout palette run presents wallpaper and bar at exactly the same reported
timestamp; the picker follows **3.008 ms** later. Selection→active topic is
242.936 ms, selection→wallpaper/bar presentation 493.722 ms, and the picker
follows at 496.730 ms. This includes the existing helper/prepare pipeline;
prewarming removes extraction misses, not all selection latency.
The matched screenshot run reports one 26 ms sampling interval between wallpaper
and picker crop progress (before: 997 ms); its direct presentation feedback
puts all three at one timestamp. Screenshot spawn and presentation clocks are
kept separate. `refresh_ns=0` supplies no physical refresh/scanout proof.
[Exact correlations and all bounded samples](evidence/0123/final-review-analysis.json).

| Default CPU bar / GL panel | First hydrated draw, ms | Render wall p50 / p99, ms | Client callback interval p99, ms |
| --- | ---: | ---: | ---: |
| First-ever open | 224.466 | 0.131 / 3.338 | 16.064 |
| Warm reopen | 10.022 | 0.120 / 2.621 | 16.660 |
| Reopen after 125 s | 9.632 | 0.099 / 1.933 | 16.039 |

Cold scene acceptance occurs at 126.305 ms, hidden GPU preparation takes
95.719 ms, and first presentation feedback follows at 228.279 ms. No interim
CPU/loading buffer is presented during that preparation. It remains first-ever
readiness latency; subsequent opens keep the prepared renderer and images.
Warm feedback deduplication prevents assigning later callbacks to the first
cached draw. The visible render tails are below 8.333 ms in these short windows,
but 16 ms callback/frame intervals remain: no physical 120 Hz or zero-drop
acceptance is claimed. CPU fallback’s longer rounded-paint tails remain above.

## Temporary duplicates

None added. The optimistic preview raster/token path is removed; the bounded
selection channel remains because superseded activations still need identity fences.

## Intermediate failures retained in the record

A first lazy-GL candidate still presented a CPU frame and then stalled for
97.718 ms while constructing GL state. Its warm/expired render p99 was already
2.340/2.039 ms. GPU-first hidden preparation fixes that conversion sequence;
preparation time is reported separately rather than hidden in visible-frame
statistics. Its bar and wallpaper presented together, but the picker followed
16.166 ms later, missing the nominal 8.333 ms synthetic schedule. This led to
the shared bar/companion palette preparation fence.
The forced-main-GL experiment had a missing search placeholder and a 33.999 ms
bar/wallpaper/picker span; it is not the shipping default or a parity acceptance.
The default CPU bar plus GL companion displays the placeholder correctly.
[Intermediate default clocks](evidence/0123/pacing-auto-analysis.json),
[forced-main-GL clocks](evidence/0123/pacing-gl-analysis.json).

## Final default renderer memory

Five alternating control/candidate pairs use the same CPU-bar/GL-panel CLI flags,
output geometry and final Nacre helper. Control retires its lazy GL context;
candidate keeps the prepared context and layer. All ten arms pass containment,
geometry, host swap-in and process SwapPss guards; swap used is zero. The catalogue
contains five generated images, so this is not a maximum-collection stress proof.
Candidate includes independent widget/clock changes: these compare complete trees.

| Unreclaimable MiB, CPU bar / native GL panel | Control median | Candidate median | Paired median delta |
| --- | ---: | ---: | ---: |
| Never opened | 37.605 | 37.816 | +0.266 |
| Closed after 125 s | 53.141 | 356.680 | +303.539 |
| Open | 127.717 | 328.930 | +201.271 |
| Close + 0.6 s | 127.961 | 296.817 | +169.122 |

The never-opened ≤1 MiB budget and +0.5 MiB regression median pass. Its five deltas
are +0.359/+0.148/+0.285/+0.266/−3.996 MiB, with no exclusions.
Warm retention exceeds the regression allowance by a large margin; it is an
explicit SMOOTHNESS FIRST spend, not a memory optimization. At 125 s the paired
component medians are +40.293 MiB anonymous, +0.094 MiB SHM and +263.152 MiB DRM;
SwapPss is zero. DRM is measured, not imputed. This removes the measured blank
expired open and 97.718 ms visible GL promotion hitch. The roughly 304 MiB warm
cost remains a material limitation for owner review; no laptop-cap claim is made.
Informational clean-file PSS medians control→candidate are 19.467→19.087 MiB idle,
39.118→38.204 expired, 39.911→38.204 open and 45.554→44.276 closed.
[All five pairs and components](evidence/0123/final-memory-analysis.json),
[raw guards and phase records](evidence/0123/final-memory-raw.json).

## Spotlight-specific palette and fallback proof

A separate open-Spotlight run uses the corrected fallback release (`cbe60901`,
SHA256 in provenance). Selection→active topic is 245.028 ms; acceptance is
477.931 ms. Target wallpaper and tagged bar update 13 present together at
498.682 ms; tagged picker update 7 follows at 501.983 ms: **3.302 ms separation**.
These are matched target revision/UI update feedback clocks, with `refresh_ns=0`.
The later same-palette update is not substituted for the earliest matched result.
This meets the nominal one-frame separation target in the private nested proof;
it does not prove physical scanout or zero dropped frames.
Private moving-image burst/WebM validation passes with the existing isolated
mpv runtime; the first attempt used its wrong library directory and was rerun
successfully. No host installation was needed.

The corrected forced-GL-unavailable run records exactly one failure event and one
GPU→CPU transition, with no retry. Its first content commit/frame/present are
127.029/129.199/130.923 ms; warm cached render is 11.652 ms. Both corrected fallback
and motion capture guards complete with return codes zero, child membership
verified and no OOM. CPU animation frame tails remain the earlier measured limit.
[Spotlight and corrected fallback clocks](evidence/0123/final-spotlight-analysis.json),
[functional guard receipts](evidence/0123/functional-guards.json).
