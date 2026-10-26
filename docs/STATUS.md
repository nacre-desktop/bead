# Status

## 2026-10-26 — B0148 cache admission (synthetic checks pass; real gate blocked)

Cache-admission failures no longer log as UI errors; retained panels keep drawing
through the current backend and GL promotion no longer waits behind relayout. The
24-image 4K CPU snapshot regression stays within 40 MiB and renders at 4320×2700.
Nested CPU/GL QML pairs pass with generated inputs, zero UI_ERROR and zero OOM.
One cold GL open remained blank at 3 s settle and showed at 8 s; first-frame
acceptance remains open. The owner's wallpaper directory is empty, and
cross-owner eviction plus pointer hover/pressed parity remain unverified.
[Scope and evidence](specs/0148-cache-admission.md).

## 2026-10-26 — B0145 idle video renderer release

The B0142 renderer now releases after 60 seconds with static wallpapers across
all outputs; video selection and an open picker hold it warm. The private nested
desktop check found the renderer gone after the grace and captured the poster
63.8 ms after video selection. Full remote CI passes. The desktop lacks libmpv,
so motion decoding was unavailable during this poster-focused check. [Evidence](specs/0145-video-idle.md).
## 2026-10-26 — B0146 live wallpaper switch latency (blocked)

The owner's live static switch samples span 412–1558 ms, while earlier nested
tests used small fixtures. Bead now emits selection-relative switch phases with
source byte count and dimensions, including the SHM upload boundary. The 17-file
real wallpaper set measured 71.6 MiB in the desktop copy (9 PNG, 8 JPEG). A
34-run release decoder-only cohort at 2880×1800 scale 1.5 measured 60.0 ms p50,
379.5 ms p95 and 379.8 ms p99; it excludes daemon, upload and presentation.
The private catalogue returns 17 entries, but wallpaper activation fails during
palette publication and produces no new active revision; QML's UI-provider check
also times out. End-to-end latency and the cache gate remain unmeasured. Full
remote Bead CI passes. No live deployment.
[Scope and evidence](specs/0146-live-wallpaper-latency.md).

## 2026-10-26 — B0144 clock publication across busy boundaries

Fixed a service timer cancellation gap that could skip minute updates without
suspend. The private-bus regression fails before and passes after; real clock
widget damage, six-minute nested idle captures, retained GL suspend/reopen and
short process-stop/private-resume proofs pass. Full remote CI passes. The old
live journal cannot conclusively identify the reported occurrence. No live
deployment. [Evidence](specs/0144-clock-frozen.md).
## 2026-10-26 — B0142 warm video wallpaper switching

One long-lived video renderer is reused through `loadfile replace`; poster shows
at once and fades into motion; `WALLPAPER_SWITCH_LATENCY` journal marker added.
Nested p95 click-to-feedback 142-153 ms (medians 123-147 ms), was 0.5-1.1 s.
Warm renderer costs about 7.7 MiB while video is active and stays resident
after switching to static. [Evidence](specs/0142-video-switch.md). No live deployment.

## 2026-10-26 — B0139 dropdown-open native-window regression

Native pearl admission no longer runs on dropdown open. It starts only with an
accepted docked drag press; no-detach release, panel close and reset cancel it.
CPU/GL each pass 20 control + 20 top-dashboard open/close cycles with zero
native requests/windows, and 20 detach image trials with no blanks/reversals.
Full remote CI passes. Capture-loaded handoff timing still misses 33 ms; no new
latency claim. [Evidence](specs/0139-prewarm-toplevel.md). No live deployment.
## 2026-10-26 — B0141 scrolling follow-up

The old allocator abort is reproduced in Luau0.740's standalone parallel analyzer,
not the Bead --check process. Upstream0.742 fixes the shared error-reporter race;
100 project checks pass with the real glibc debug allocator, and CI now pins that
release. Runtime Luau is unchanged. The30-minute runtime scroll soak remains open.
Cache retirement passes70 renderer tests; native GL80 memory and direct120Hz AI
proofs are in progress. Picker shares B0134's40MiB budget, and open history/popup/
Clipboard require measured justification at≤2MiB each. No live change.
[Scope, sources and gates](specs/0141-scrolling-followup.md).

## 2026-10-26 — B0131 warm covered release

Niri admits the detached target before drag and reuses cached content and the GL
context. Separate 20-trial presentation cohorts pass: CPU/GL handoff p95
31.12/31.39 ms and render p99 9.22/2.95 ms. Separate image cohorts are 20/20
blank- and reversal-free; capture-loaded p95 36.68/34.83 ms still fails 33 ms.
QML's 180 ms OutCubic return and control/AI detach–dock checks pass.
Five-pair UI idle/closed deltas pass the 1 MiB budget; active warm memory is spent
explicitly (CPU detached +8.21 MiB, GL docked +21.21 MiB). Private compositor DRM
is frame-history-sensitive; no combined desktop cap or physical 120 Hz/17 ms
claim. Full remote Arch CI passes. [Evidence and limits](specs/0131-pearl-release.md).
No live change.

## 2026-10-26 — B0129 launcher icons on the first frame

Five alternating private CPU pairs show zero missing-icon presentations on all
cold, warm and expired opens. Median first-presented deltas are -1.040 / +0.081 /
+0.713 ms; first-key echo UI CPU max is 3.928 ms. The largest phase median
unreclaimable cost is +2.895 MiB against the explicitly allowed 4 MiB budget.
Full remote Arch CI passes; native generation markers verify actual panel-worker expiry.
[Evidence, spreads and remaining QML differences](specs/0129-launcher-icons.md).
Final search-results UI CPU remains 10.917 ms median; the 8 ms gate above measures the first echo.
Startup excursions exceed 4 MiB; acceptance uses five-pair phase medians, not peak residency.
No live desktop change or laptop absolute-memory acceptance.

## 2026-10-26 — B0136 small parity fixes

Notification adapters retain app identity, favourite markers use the filled glyph,
and Today matches the wallpaper action row. The published daemon owns bounded
Previous history and availability; Bead consumes the published contract. Full
remote Bead/Nacre CI and official jobs pass, including the real pointer-release
regression found during capture. Five-pair CPU memory fails: idle +0.516 MiB exceeds the
+0.5 limit; hovered favourites +1.703 MiB exceeds the declared 1 MiB budget.
All 13 final Today states and the matched static Colors pointer pair pass.
Opening smoothness remains unproven; [the spec](specs/0136-small-parity.md) records
source-pinned pairs and remaining visual gaps. No live change.

## 2026-10-26 — B0134 picker retention and confirmed handoff

Five alternating private CPU-bar/GL-picker pairs bound static closed retention to
28.868 MiB above idle; all relative phase medians pass +0.5 MiB. Static wallpaper/bar
and picker palette present at 124.341/127.925 ms median. Video wallpaper/bar is
147.363 ms, but picker palette is 150.743 ms and fails 150 ms in three pairs.
Warm drawing passes 16 ms; expired first-presentation feedback is 17.956 ms and
fails. Cold presentation is 107.430 ms. Compressed CPU snapshots retire exclusive
GL pools while confirmed palette/pixel staging overlaps the unchanged activation RPC.
Full remote Arch and official Bead/Nacre CI pass. Frozen measured source `c63acedf`
and integrated `588d9506` are distinct. [Evidence and limits](specs/0134-picker-retention.md).
No live deployment, physical scanout or laptop-cap acceptance.

## 2026-10-26 — B0137 shared edge-panel hover

AI and control panels share lip/body/focus exit grace. Backdrops preserve stable
lip input and artwork, replacing the remap race. Final private CPU proof passes
20/20 trials per panel with ≥5.12 s body holds, exit closure and composer focus
hold. Full remote Arch CI passes; paired QML states and regenerated goldens were
inspected. Animation/content parity limits and unmeasured memory are explicit in
[the spec and evidence](specs/0137-ai-hover.md). No live change.
## 2026-10-26 — B0135 shared scrolling

Finger input follows pixels directly; real axis_stop starts kinetic motion,
opposite input cancels old travel, and wheel steps retarget over 125 ms.
Shared virtual lists and viewport/buffer repair reduce repeated frame work.
Full remote Arch CI passes on integrated `b6c61725`. Settings side-tab wheel
render p95 improves 15.59–16.15→1.29–1.51 ms. Bounded Settings content renders
in 6.78 ms, but reversal remains 29.25 ms (older diagnostic cohort). Direct private finger-stop reversal
is 11.35 ms; neither meets one 120 Hz frame. The GL80 picker now reopens without
cache errors using demand-driven 256 MiB global admission; sampled active process
unreclaimable memory reaches 528.64 MiB including DRM. This is a documented stress
trade. Five CPU pairs fail history/Clipboard open budgets at +1.484/+2.176 MiB;
popup open is +1.258 MiB. Idle/closed gates pass, and picker8 open saves 36.606 MiB.
The AI follow correction now permits small finger movement; native CPU reversal
is 49.22 ms and render p95 2.093 ms. Required-priority full CI passes from a
fresh target; an earlier allocator abort remains unexplained and documented.
120 Hz/no-drop acceptance fails. No live deployment or laptop-cap claim.
[Source pins, per-surface results and known parity gaps](specs/0135-scrolling.md).

## 2026-10-25 — B0132 Today controls

Today has the Colors button and clickable palette dots through the existing app
route. Media transport retains its geometry, follows live player state and uses
QML's disabled opacity. Fake-daemon calls, four goldens, full remote CI and paired
private interaction captures pass. Five-pair CPU unreclaimable deltas pass the
+0.5 MiB gate: idle +0.168, open +0.117, closed +0.066 MiB (unit-only snapshot).
[Evidence and remaining parity limits](specs/0132-today-controls.md). No live change.

## 2026-10-26 — B0126 instant drag shipped through 27abdf62

The integrated CPU build passes the current QML-relative gate across three
alternating pairs per rate: Bead p95 26.271/11.764 ms at 60/120 Hz versus QML
26.801/11.823 ms. Bead p99 is 26.517/11.875 ms, within two frames. The first
masked frame falls from 12.095 to 0.562 ms by preserving the generated frame
image and text rasters; unchanged placement commits and delayed local-value
flushes are removed. Local labels skip Luau/layout; shared fill and handle agree
in all 384 recorded visual paints and 84/84 samples per implementation.

Five-pair CPU unreclaimable deltas are −0.055/+0.156/−0.867/+0.270 MiB across
idle/open/post-drag/closed, within +0.5 MiB and the 1 MiB feature budget. Fresh
fixture first-key CPU is 7.110 ms, below 8 ms. Native AMD GL p95 11.921 ms is
slower here; controls keep their CPU default. All five stages passed full remote
Arch and exact-commit GitHub CI; final GitHub CI passed an unchanged-source retry
after an unrelated Widgets spawn assertion.

Later-main first-key CPU regressed to 9.254 ms median when native preparation
resized the source on the key. `27abdf62` primes that existing buffer on accepted
idle frame credit: all three keys pass (median 7.346, max 7.374 ms), with no
key-time resize. Fresh drag p95 is 26.274/11.764 ms, p99 26.382/11.939 ms;
182 native paints and 41 ROI images remain coherent. Five additional memory
pairs pass (largest phase median +0.080 MiB). Full remote Arch CI passes.

These are private synthetic labwc feedback results at queried 2880×1800, scale
1.5. Physical scanout, Intel/laptop latency, absolute laptop memory and complete
page parity remain open. Earlier screenshot-stress tails remain recorded.
[Measurements, paired five-state images and limits](specs/0126-instant-drag.md).
No live desktop or installed configuration changes.

## 2026-10-25 — B0131 covered pearl release

The dragged layer remains visible until native placement succeeds and its first
content frame commits; stale geometry cannot uncover a window at the wrong pose.
Re-docking uses the scene's easing after its first covered destination frame.
The final private CPU cohort passes 20/20 sampled releases with no blanks or reversals.
Pacing acceptance remains open: render preparation has long outliers and software
feedback has no refresh timing. No live deployment. [Evidence and limits](specs/0131-pearl-release.md).

## 2026-10-25 — B0133 Welcome fixes

Welcome and app headers use themed mother-of-pearl artwork. The four bundled
scenes and 64 palettes are ready before the first frame; decoded pixels cost
12.23 MiB in the on-demand app. The 1100×800 minimum fits all five tour pages.
Welcome follows the shortcuts topic, with readable grouped rows and late updates.
Full remote integration CI passes; native QML/Bead page pairs are reviewed.
QML’s migrated demo-start command remains broken in the reference; inherited
parity limits are explicit. [Evidence](specs/0133-welcome-fixes.md). No live deployment.

## 2026-10-25 — B0125 fullscreen edge lips

Edge lips now share the bar/frame's Top layer, so niri fullscreen clients cover
their artwork and pointer regions. Notification cards follow the chrome through
a Luau per-view policy; explicit level OSD retains Overlay, matching QML.
Matched private QML/Bead captures verify normal hover/click, fullscreen pixels,
mouse and keyboard delivery at all three edges, restoration and overlay policy.
Full remote Arch CI passes, including all renderer, lifecycle and license graphs.
[Evidence and limits](specs/0125-lips-fullscreen.md). No live deployment.

## 2026-10-25 — B0123 Spotlight and wallpaper palette

Spotlight is a centered overlay over the dimmed live desktop. First publication
waits for visible thumbnails; its single prepared layer/worker survives the old
120-second expiry. The existing GL panel renderer stays prepared, with CPU
fallback when runtime GL is unavailable. Nacre prewarms bounded existing palette
markers and removes the native observer’s 500 ms publication delay.
Wallpaper and bar present together in the clean nested run; the picker palette
follows 3.302 ms later with Spotlight open (3.008 ms in Carousel). Prepared
warm/125-second reopens draw in about 10 ms.
First-ever GPU preparation takes 95.7 ms before the fade begins. Full remote CI,
four regenerated goldens and private QML
pairs validate the implementation. Five-pair never-opened cost is +0.266 MiB;
125-second retention costs +303.539 MiB, mostly DRM, under SMOOTHNESS FIRST. CPU
painting still has long frames. [Measurements, sources and limits](specs/0123-picker-spotlight-palette.md).
No live deployment or laptop-cap acceptance.

## 2026-10-25 — B0124 pill clicks and compact updates

The status pill background and gaps are inert; widget-owned targets retain the
click default. Updates now has a content-sized count/checked header, health,
existing updater/refresh routes and an optional bounded package list for N0077.
Full remote CI stages pass; ten goldens and private paired interaction captures
verify the layout and routes. The sampled CPU development-build reveal remains
later than QML; animation/latency acceptance is not claimed.
[Evidence and limits](specs/0124-pill-click-updates.md). No live deployment.

## 2026-10-25 — B0109 wallpaper picker repairs (review candidate)

Toolbar layouts and Dynamic persist through the published provider contract;
carousel travel follows QML's 300 ms OutCubic curve. Generation-fenced previews
remove the palette-confirmation wait, video replacement reuses mpv's context,
and Current scene loads its preview/name/kind lazily and opens the picker.
The cold background t10 sample improves from 806 to 154 ms (paired QML 307 ms).
Full remote Arch CI and final fifteen-state integration checks pass; paired captures
cover six video formats. All five-pair CPU memory gates fail: closed deltas are
+0.586–1.156 MiB except Dynamic +11.301 MiB, including +9.887 MiB SHM consistent
with a spare wallpaper slot. This is an explicit SMOOTHNESS FIRST/SPEND trade,
not memory acceptance. [Evidence and remaining visual limits](specs/0109-picker-fixes.md).
No live desktop, installed software, physical scanout or laptop-cap acceptance.
## 2026-10-25 — B0114 resumed under feature budget

The lead accepted the measured 0.555–0.820 MiB median widget cost against a
2 MiB always-on feature budget. The candidate is rebased onto current main;
sidebar hover-state isolation, parity and staged pushes are in progress.
The focus fix is retained. Captures remain private/nested; no live deployment.
[Evidence and limits](specs/0113-widget-system.md#phase-2-candidate--b0114-staged-integration).

## 2026-10-25 — B0119 keyboard focus fixed

Closed warm panels no longer reassert exclusive keyboard focus. Matched private
niri runs deliver keyboard input 100/100 times versus control 0/10; the 4 Hz title
spinner costs 0.400% core, with no per-title backend log or same-width title layout.
Full remote CI passes; widget-host and B0120 changes are preserved. No live change.
[Cause, measurements and traces](specs/0119-click-focus.md).

## 2026-10-25 — B0111 lead-accepted integration

Notifications share the permanent daemon source; sidebar hover/cached visibility
and caption-centred dashboard pearls are accepted for integration with B0113 widgets
and B0120 spacing, retaining B0119 focus fixes. Full remote Arch CI passes on
`f6e4e85` (608 nacre-shell tests passed, zero failed).
lead-accepted 2026-10-25: Today/Quick closed deltas +0.6..+2.6 MiB; consolidated memory review follows
Failed measurements and keybind/peek proof limits remain in the
[spec and evidence](specs/0111-live-bugs.md). No live deployment.

## 2026-10-25 — B0120 pill spacing and wallpaper backdrop

Status widgets now use optical-width tokens: native gaps are 17–19 physical px
at scale 1.5; counts and clock align with their icons. The B0113 port matches
all four accepted goldens byte-for-byte and has fresh 4× native comparisons.
Matching Bead's wallpaper namespaces in both Nacre niri entry points removes the
measured workspace dark band for static and supervised video (12% → 0% dark rows).
Video uses its poster in the backdrop; other evidence limits are explicit in
[B0120](specs/0120-pill-wallpaper-flash.md). Full remote Bead and Nacre CI pass.
No live deployment.

## 2026-10-25 — B0113 phase 1 gates pass

The bar now composes validated Luau widget packages through a general strip host.
All four native bar states are pixel-identical to control, and all required raw
QML pair RMSE gates pass. Five alternating CPU pairs pass at
+0.133/+0.113/−0.492/+0.121 MiB unreclaimable median (idle/open/typed/early closed);
first-key UI CPU peaks at 3.857 ms. Full remote CI, unchanged bar goldens and
isolated release checks pass on `a9c2f75`, retaining B0118 from `2e523f2`.
The QML/daemon reference is refreshed to `421da0f`. Nacre opaque `widgets` store
registration is requested; phase 2 remains separate. All captures are private
and nested; the laptop absolute cap remains for the lead.
[API, full measurements, fixture details and paired crops](specs/0113-widget-system.md).

## 2026-10-25 — B0118 daemon recovery

Bar status now follows reconnecting daemon sources; workspace origins reuse the
notifications compositor subscription. Wallpaper consumers survive a late daemon.
Full remote Arch CI passes. [Scope and recovery proof](specs/0118-daemon-reconnect.md).

## 2026-10-25 — B0108 title/tabs, lead-accepted

The local candidate centers measured title/tab groups, retains a real window title
through panel focus loss, and switches dashboard pages in the resident surface.
Full remote Arch CI passes on `72b8498`; 40 updated goldens were inspected.
The initial GitHub run exposed host-font-dependent measurements; session-owned
font policy and bundled harness fonts now pass unchanged Ubuntu dashboard/bar
goldens as well.
Five alternating CPU pairs measure Media at +0.513672 MiB unreclaimable median
(+0.500 MiB limit); all other phases pass. **Lead-accepted 2026-10-25:** the lead
judged the 14 KiB excess within measurement noise and authorized integration.
Native full series are from visual candidate `da5473b`; a final-code `4a4ed0c`
quick title/Media refresh passed after acceptance. Rebase was a no-op.
[Candidate, failed measurements and proof limits](specs/0108-title-tabs.md).
No live desktop, installed software or laptop-cap acceptance.

## 2026-10-25 — B0106 dashboard follow-up, review candidate

First prepared content commits at 57.2 ms instead of 271.0 ms in matched release CPU runs.
Five alternating pairs pass closed unreclaimable recovery: Today/Media/Weather/System
+0.160/−0.039/+0.059/−0.125 MiB. Open growth is not fully isolated to image bytes and remains unaccepted.
Cold image-distance t10 median is 113.6 ms versus QML 119.8 ms, but one pair is +50.2 ms late;
warm/close motion remains late. Media, Weather and System RMSE improve; Today is unchanged.
Full remote Arch CI passes; source-pinned evidence and remaining gaps are in
[B0106](specs/0106-dashboard-followup.md). No live change or laptop-cap acceptance.

## 2026-10-25 — B0107 slider tracking accepted

Continuous kit arguments and staged release handling remove hit-cell snapping and late-echo
snapback. At 2000 px/s the revised moving pixel-error p95 is 155.0 px versus QML
156.5 px and Bead control 132.0 px; all twelve final release/write checks pass.
Five alternating memory pairs pass the amended median gate; first-key CPU stays ≤8 ms.
Full remote Arch CI passes. No live desktop/installation changes.
[Measurements, source hashes and parity gaps](specs/0107-slider-tracking.md).

## 2026-10-25 — B0105 latency investigation blocked

A one-replacement frame policy leaves CPU hover at 32.09 ms and drag at 66.19 ms,
above the 24.7 ms target. QML in the same nested chain is 95.72/98.13 ms hover/drag.
Changing only the private parent to 120 Hz halves control hover while niri still
advertises 60 Hz; that diagnostic is not a canonical-gate or physical-scanout pass.
Three Home pairs pass first-key CPU, but an unexplained +1.516 MiB anonymous closed
outlier triggered the memory stop and the queued GL candidate was canceled.
The engine experiment is retained as a rejected patch. Only the opt-in QML probe
and [source-pinned measurements](specs/0105-present-latency.md) are integrated; full
probe-only remote Arch CI passes. No live desktop or installed software changed.

## 2026-10-25 — B0104 pearl drag/close (integrated, lead-accepted)

Logical relative dragging, native close, bounded recovery and view-state persistence
are implemented. **lead-accepted 2026-10-25, follow-up memory attribution**:
the unclosable-pearl fix takes priority over detached/closed CPU deltas of
+0.891/+0.828 MiB for this unit, including the +2.133 MiB anon outlier.
Full remote Arch CI and GitHub checks pass on integrated `8a16336` (base `b41d636`). Ctrl+W/native close
requests pass; synthetic Super+Q and final move/dock/stale/GL refreshes remain gaps.
One current-base diagnostic pair records detached/closed +0.531/+0.320 MiB,
with allocator in-use growth and anonymous-backing variance; no new gate pass or code optimization is claimed.
[B0104](specs/0104-pearls-drag.md) preserves failed gates and proof limits.
No live change or laptop-cap claim. Original images stay private because updates were unstubbed.

## 2026-10-25 — B0097 panel anchors and motion (review candidate)

Launcher/control contours reveal from the bottom/right frame; status routes keep
invoking-icon centres and six hover previews use QML sizing/content/intent delays.
Private 2880×1800 scale-1.5 pairs and 25 ms motion strips pass their capture gates;
bar/lip/private-key drawer consistency is 0.000% ROI RMSE. Three CPU typing pairs
pass at 5.750 ms first-key median and +0.117/+0.070/+0.219 MiB idle/open/closed
unreclaimable deltas after removing unnecessary per-shadow asset copies; the
initial +1.496 MiB closed-memory failure remains recorded. Full remote CI and
goldens pass on the integrated source. [B0097](specs/0097-panel-anchors-owner.md)
records measured-source boundaries and residual launcher timing/body/calendar
corner gaps. No live change or absolute laptop-cap claim.
## 2026-10-25 — B0103 integrated; nested latency targets remain open

Local slider feedback, motion coalescing, bounded host writes and keyboard-only focus styling
are integrated at 417ac32/7ac5ef9; measurement tooling is 60c82df. Full remote Arch CI passes.
Private CPU/GL drag p99 falls from 1225/1454 ms to 70/78 ms, while hover remains about 32 ms.
All four absolute 24.67 ms latency gates fail; measured surface commit→present dominates.
Six populated Home pairs give first-key CPU 6.802→7.101 ms and median idle/open/closed
unreclaimable deltas +0.037/+0.203/−0.014 MiB. The +2.496 MiB closed outlier was not repeated
in three allocator follow-up pairs; live heap/cache sizes stay close, with anonymous residency spread.
[Evidence and explicit QML gaps](specs/0103-control-latency.md) include paired screenshots and stages.
No live test, installation or physical laptop timing/cap acceptance is claimed.

## 2026-10-24 — B0098 sidebar and Settings (review candidate)

The sidebar now uses QML-shaped controls, Pin and an active external-panel lip.
Private pointer probes verify pin/outside/unpin dismissal. Settings opens at
1100×800 (minimum 544×480); shared scrolling applies QML's pixel multiplier and
wheel easing while keeping non-content gestures raw. Matched evidence, memory,
full remote Arch CI, an unresolved transient memory sample and explicit
physical-wheel/integration limits are in
[B0098](specs/0098-sidebar-settings-owner.md). No live change or laptop acceptance claim.

## 2026-10-24 — B0103 control input (blocked candidate)


## 2026-10-24 — B0103 control input (blocked candidate)

The candidate coalesces queued pointer positions, previews slider values locally,
limits host writes, and separates pointer styling from keyboard focus. Remote
Arch CI passes. The private capture lease remained unavailable for over 90 minutes;
CPU/GL presentation, matched QML, first-key and memory gates are all unmeasured.
[B0103](specs/0103-control-latency.md) records the candidate and remaining proofs.
No integration, live test or performance acceptance.
## 2026-10-25 — B0102 dashboard fixes (review candidate)

The bar-only `status.hovered` reload failure is fixed in published `ac194d7`;
companion tab commands now dispatch through the host after lifetime validation.
Today restores its wallpaper and calendar/host/media columns. Full remote Arch
CI and seven-state private QML/CPU-Bead captures pass their functional checks.
The relative memory gate fails: Today open +7.551 MiB; closed Media/Weather/System
+1.055/+0.945/+1.195 MiB. Memory and timing parity remain unaccepted. [B0102](specs/0102-dashboard-owner.md) contains the evidence and remaining
Media/Weather/System and visual-polish gaps. No live change or laptop-cap claim.


## 2026-10-24 — B0103 control input (blocked candidate)

The candidate coalesces queued pointer positions, previews slider values locally,
limits host writes, and separates pointer styling from keyboard focus. Remote
Arch CI passes. The private capture lease remained unavailable for over 90 minutes;
CPU/GL presentation, matched QML, first-key and memory gates are all unmeasured.
[B0103](specs/0103-control-latency.md) records the candidate and remaining proofs.
No integration, live test or performance acceptance.
## 2026-10-25 — B0100 wallpaper picker (review candidate; memory fails)

Super+W opens a lazy Luau carousel, spotlight or hexagon picker with search and
revision-fenced image/video activation. N0070 supplies the catalogue contract.
Full remote Arch CI and thirteen guarded private desktop states pass. Paired
review fixed fullscreen placement, bar overlap, captions and search styling.
Five final CPU control/candidate pairs give unreclaimable paired median deltas
+0.102 MiB idle, +0.617 carousel closed, +0.586 spotlight closed and +0.633 hexagons
closed. The closed phases fail the +0.5 MiB gate; the excess is anonymous and its
residency attribution remains incomplete. Product optimization stops at the gate.
Three populated-Apps typing pairs remain below 8 ms UI CPU per key.
[B0100](specs/0100-wallpaper-picker.md) preserves the failed cohorts, rejected
experiments, paired RMSE/reveal evidence and poster/motion/import gaps.
No live desktop change or absolute laptop-cap claim; pushed for lead review.


## 2026-10-24 — B0096 owner bar parity (review candidate)

Status values now share icon hit areas and centered line boxes; the default tray
visibility, pearl geometry/app alignment and dashboard dwell follow the QML
reference. Full remote Arch CI passes. Five private same-host paired cycles give
open/closed unreclaimable median deltas −0.314/+0.375 MiB (CPU bar/native-GL
dashboard), within the relative gate. [B0096](specs/0096-bar-parity-owner.md) records
paired 2× crops, RMSE and remaining compositor hover-cue/font/updates-popout gaps.
No live desktop change or laptop-cap claim.


## 2026-10-24 — B0080 populated Home first key (review)

Five matched CPU pairs now measure Home → completed search at 7.598 ms
(control 15.161); standalone is 6.508 ms and later keys stay below 8 ms.
Opening improves by 0.683 ms; settled idle/open/closed memory deltas are
−0.165/+0.039/−0.316 MiB. Full remote Arch CI passes. Projection coalescing,
bounded pure-markup reuse and partial SHM adoption preserve settled pixels.
[B0080](specs/0080-home-first-key.md) records stage costs, paired images and
fixture/nested limits; no live switch or real-engine/laptop claim.

## 2026-10-24 — B0079 extras placement and reveal (review)

Windows, Commands and Keyboard shortcuts now use QML’s centered bottom
attachment and 300 ms OutCubic reveal, with token-controlled edge and geometry.
Search fields keep the filled background without a focus outline; Overview
reuses the bar’s category glyphs. Final full-frame RMSE is 7.803%/4.369%/2.851%
(from 25.606%/29.112%/30.109%). Full remote Arch CI passed after B0075 integration;
matched private captures, corrected Overview motion sequences and residual gaps
are in [B0079](specs/0079-extras-placement.md). No live change or new memory claim.

## 2026-10-24 — B0075 first-key recovery (review)

Cold opening motion left a terminal pose and old SHM repairs for the next input.
Five current-main CPU pairs now measure first key 7.259 vs 17.953 ms; later
`a`/BackSpace medians are 6.379/7.505 ms. Paired idle/closed memory deltas
+0.054/−0.067/−0.278 MiB and opening +0.066 ms pass; full remote Arch CI passes.
The separate Home-bar first key remains 10.017 ms. Scope, spreads and failed
prototypes are explicit in [B0075](specs/0075-first-key-regression.md). No live switch.

## 2026-10-24 — B0077 extras surfaces (review candidate)

Overview, Commands and Keyboard shortcuts now have independent lazy Luau surfaces,
replacing the mode aliases to the launcher. Overview groups daemon windows by
workspace and focuses through N0040; shortcuts use N0051; the palette uses opaque
menu-engine activation. Closed workers release their subscriptions and payloads.
Capture handles, semantic window moves and an engine route matching QML Palette
content/order remain explicit asks in [B0077](specs/0077-overview-palette-keybindings.md).
Full remote Arch CI and all native error/swap gates passed. Aligned-panel RMSE:
Overview 3.652%, Commands 8.832%, shortcuts 5.058%; full-frame values and paired
images are in the spec. Exact palette content parity remains blocked; no live install.

## 2026-10-24 — B0076 docked device details (review)

Wi-Fi, Bluetooth, battery and audio now reuse their popout bodies inside the
control dock. Unsupported tiles hide and unsaved order follows QML’s catalog.
Shared modules remain part of atomic reload; page actions and lease retirement
are tested. Full Arch CI and private paired captures are recorded in
[B0076](specs/0076-docked-detail-pages.md), with explicit residual visual gaps.
No live change or new memory-acceptance claim.

## 2026-10-24 — B0074 dashboard first reveal (review)

The reveal clock now starts at admission and transient contour SVGs use content-sized
rasters. Three matched nested CPU pairs on B0072's landed base: cold 10% progress
201/256/197 ms and warm 200/250/205 ms (QML/control/candidate). Median differences
meet ±20 ms. Relative UI memory medians idle/open/closed: −0.029/−0.235/+0.262 MiB;
full remote Arch CI passed. First-key CPU remains above 8 ms in both builds
(17.991/17.915 ms); no launcher changes. The 90% image proxy remains ~25 ms later
than QML. Method, spreads and remaining limits: [B0074](specs/0074-dashboard-reveal-delay.md).

## 2026-10-24 — B0072 launcher Quick controls (review; CPU gate fails)

The launcher now projects DND, night light, stay awake and a known updates count
from scoped daemon topics, reusing control-centre calls. Luau owns layout and
release activation preserves search focus. Full remote Arch CI, fake-daemon
lease/call tests, goldens and matched private QML/Bead captures pass.
Five desktop CPU pairs against B0073/main: first key 18.359 vs 17.656 ms (**8 ms
fails in both**); flip keys 6.670/7.952 ms; paired open delta −0.116 ms; final
closed PSS+DRM+SwapPss delta +0.044 MiB (memory passes). No live change.
See [B0072](specs/0072-launcher-quick-controls.md) for source gaps and evidence.

## 2026-10-24 — B0073 status controls and parity (review)

Status popouts now share QML switch/button styling, network empty-state metrics,
Bluetooth ordering/density, guarded audio/profile actions and weighted OSD text.
Nested CPU Wi-Fi RMSE 10.248→6.195%, audio 7.200→5.482%;
three bar-only pairs add a median 0.027 MiB PSS+DRM+SwapPss. Full remote Arch
CI passed. Updates is configurable but awaits its live daemon count; dashboard
first-reveal delay remains. Details and known gaps: [B0073](specs/0073-visual-parity-6.md).
No live or launcher changes.

## 2026-10-24 — B0071 visual parity round 5 (review)

Dashboard artwork now grows with rounded corners and a moving rim; four status
popouts join the top bar and share its colour. Nested CPU RMSE before→after:
Wi-Fi 17.825→10.248%, Bluetooth 15.277→5.532%, battery 14.339→4.846%, audio
19.316→7.200% with matched deterministic starting state. Pointer captures verify
the parent geometry and sample the same Bluetooth toggle; brightness/keyboard
OSDs have read-only fixtures and follow the active palette. Full remote Arch CI
passed; goldens inspected. Remaining content/control/typography and ~75 ms warm
open offset are explicit in [B0071](specs/0071-visual-parity-5.md). Relative UI
PSS+DRM was not rerun. No live changes or Nacre/launcher edits.

## 2026-10-23 — B0030 notices/OSD inside the main shell (review)

Notices and the OSD are now transient panel views of the one shell process, offered by the
bar sessions on the shell's single daemon link and UI provider; the separate notice process
is gone. Relative memory gate (desktop, CPU tiny-skia, 5 alternating pairs): idle +0.381 MiB
(anon +0.07) PASS, popup shown +1.27, closed -0.52 (anon +0.25). Nested smoke 35/35. Popups
now yield to any other shown view (launcher/dashboard too). Also fixed a whole-shell failure
when all SHM buffers were still held after a popup resize. Full Ubuntu CI job on dev-2: 430
tests, fmt, clippy, boundary, features, Luau, spikes, services, deny all pass. Product lines
+415/-134, framework +66/-2. See [B0030](specs/0030-notices-in-main-shell.md). No live change.

## 2026-10-23 — B0020 notifications/OSD consumer implemented (review)

Bead now consumes the daemon's notifications producer (N0017 default-action marker, N0036
brightness level): popup stack, hover pause, DND/OSD suppression, dismiss vs forget, dashboard
history/DND controls and OSD, never owning the bus name. Private nested smoke against the real
producer: 32/32. Memory relative gate FAILS (+1.9..+2.7 MiB Pss vs the private-owner control, CPU
backend); unresolved. Single-UI-provider limit and missing swipe/bar-popout recorded in
[B0020](specs/0020-notifications-osd-product.md). No live change.

## 2026-10-22 — B0025 per-output sessions accepted

Each output now owns UI state/update/commit/input/teardown with shared services,
fonts/assets and immutable palette data. Old-scale caches/pools and idle spare
frames release; allocator slack is returned after quiet UI generations.
Matched CPU PSS deltas: Hyprland one -0.992 MiB; niri initial/1.25/return
-1.401/-0.880/-1.017 MiB.
Extra-output cost +1.862 MiB passes ~3 MiB. Named counters explain
0.678 MiB of 1.033 MiB extra live heap, leaving ~0.356 MiB unclassified.
Full Arch Bead CI and private lifecycle/input/idle/scale proofs pass. Niri's
single Winit output has no supported private hotplug route; that limitation
remains explicit. B0022's architecture blocker is cleared. See [B0025](specs/0025-per-output-ui-sessions.md).
No installed-shell change or live session.

## 2026-10-22 — B0022 bar actions on Nacre N0031 routes; review

With a daemon, the bar's workspace click/scroll, title and audio wheel, audio
mute, battery-wheel brightness and tray activate/secondary/context/scroll are
output-scoped N0031 calls (pin `652d6c4f`), enabled only while the Welcome
advertises the exact method; the tray list comes from the daemon `tray` topic.
Nothing falls back to the local compositor, audio or tray paths in daemon mode.
B0023's assumed `session.request` shape was wrong: the published reply is
`{changed:true}` and every non-lock action needs `confirmed:true`, so Bead now
also confirms Suspend. Correction: QML's active-title wheel changes volume, not
workspace. Fake-daemon tests run the real bar template for routing, capability
gating and daemon restart; a private nested Hyprland two-output proof covers the
same against a fake daemon. See [spec 0022](specs/0022-bar-composition-lifecycle.md).
No installed-shell change or live session.

## 2026-10-22 — B0021 wallpaper memory fixed; review

[B0021](specs/0021-static-wallpaper-surface.md) memory diagnosis is complete at
`ac27ca3`; full Bead CI passed in the prod Arch container. The earlier
26.64 → 59.79 MiB comparison changed CPU-only release to native GL release.
Matched CPU main/pre/post/fixed captures passed privately on both compositors.
The companion decoder removes settled GdkPixbuf/Glycin residency: fixed UI PSS
is 9.340 MiB on Hyprland and 19.526 MiB on niri, +0.251/+0.619 MiB over main
after excluding niri's additional 9.889 MiB compositor-held wallpaper SHM.
niri does not release that buffer; its documented safe fallback remains.
A private wallpaper switch returned to 10.069 MiB after a 78.653 MiB UI-only
transient peak. See [FEASIBILITY](../FEASIBILITY.md) for mapping/heap evidence,
matched flags and limitations. Ship the companion beside the UI binary.
Native GL's absolute memory is a pre-existing acceptance issue. Centre remains
unsupported. Stop for review; no install, live desktop change or push occurred.

## 2026-10-22 — B0020 producer published; default-action contract gap

[B0020](specs/0020-notifications-osd-product.md) now pins Nacre producer source
`f468060038edfc33b99a1b06bf773ea9ffd90b7e`; N0017 phases A+B and owner policy
decisions clear the former producer/policy gate. Zero-timeout popup hiding uses
the configured QML default; dismiss retains and explicit Forget deletes. History
is bounded with independent storage health. Keyboard-layout OSD is deferred.
Implementation remains blocked on a narrower published-contract gap: actions
have opaque IDs and labels, but no default-action marker/reference. QML needs it
to filter buttons and optionally invoke the default on card click. Do not infer
it from action order/labels/IDs. Upstream must publish and test the additive
metadata before repinning; no producer edit or partial consumer was integrated.
Product line delta is +0/-0. No nested test, memory delta, install or live change.
Full `.github/workflows/ci.yml` equivalent ran on source `4434768` in the
PROD Arch container: fmt, warnings-denied Clippy, workspace/trace tests, renderer
configurations, facade/headless/Luau checks, capture regressions, private services
and SNI/DBusMenu lifecycles, and all cargo-deny graphs passed. The first archive
transfer omitted Git metadata, failing two revision-dependent Python suites;
after restoring the exact Git commit, their reruns passed 10/10 and 8/8 tests.
All 28 checks passed across the full run and those environment-only reruns.
This receipt changes documentation only; it is not a product-parity claim.

## 2026-10-22 — B0019 launcher review candidate

Source `c8b55b5` adds asynchronous daemon queries, same-scope pending rows,
route/epoch fencing, favourite stars and published pin/unpin, ordered catalogue
results and group routes, keyboard activation, and focused preserved input on
reopen. Rows/tickets/actors retire after closed+idle; bounded scalars survive.
Full remote Arch CI and native private input/reopen passed on both compositors.

Niri's complete 200-key diagnostic measured 6.269 ms steady presented p99 and
25.016 ms first key; steady UI CPU p99 was 5.351 ms, first-key CPU 14.777 ms.
Hyprland's 200-key trace measured 9.871 / 25.214 ms, but its arm failed a new
post-window harness waiter; that raw arm remains invalid. The fixed two-key
contract smoke passed without repeating its timing window. Query/order identity
and RPC RTT now join through exact commit/presentation; target acceptance,
matched memory, physical timing and Gate D remain open.
The close-memory diagnostic points to allocator residency, with only +0.262 MiB
live heap; DRM remains unknown. See [B0019](specs/0019-launcher-parity.md) for
full numbers, failed-attempt receipts, raw provenance and limits. No cheap
rounded-painting change was identified. No push, live test, install or switch.

## 2026-10-22 — Laptop reference capture kit prepared

The [runbook](research/laptop-reference-capture-runbook.md) now provides a
single owner-signaled Niri command, with optional Hyprland timing. The artifact
manifest pins the recovered frozen control binary and matching frozen `shell/`
tree against the current main candidate. Exact-source Bead CI passed on dev-1.
The private desktop rehearsal completed all six Niri arms in 41m18s; geometry,
scope/OOM, host swap-in and close checks passed for every arm. Desktop DRM
fdinfo was unavailable, so its summary is **REHEARSAL ONLY** with PSS-only
relative deltas, not a laptop verdict.
Two earlier A1 rehearsal attempts stopped before valid windows while the frozen
shell bundle and UI-only fixture observer were corrected; their partial outputs
remain separate and invalid.
The laptop GPU path has not been inspected and no laptop capture was run; the
prepared reference capture still awaits the owner's signal and AC idle window.

## 2026-10-22 — B0018 status-pill routing complete

Source `c6ba4b3` groups the right-side icons in one pill and routes the four
existing device views through nonmodal hover peeks and compact top-right
control panels. Other icons retain hover/pointer behavior while peeks are
suppressed; native click switches content, pointer-away and Escape close, and
closed/idle resources retire. Shell code owns appearance and timing policy.
Requested B0017 was renumbered because 0017 already covers network controls.

Exact-source private nested CPU input and the full remote Arch CI equivalent
passed, including the existing matched QML placement/status goldens. Native
testing found missing surfaces for warmed peeks; input-bound review also fixed
shadow input over the bar. One existing watchdog deadline failed once; isolated
and complete unchanged reruns passed, without establishing a cause or relaxing
a deadline.
See [B0018](specs/0018-control-center-status-pill.md) for source/binary identity
and validation. The baseline has no edge-charge graphic; no edge activation,
new controls/providers, live switch, memory clearance, install or push is claimed.

## 2026-10-22 — B0016 tray preview ready for review; core producer gate

Source `6efca12` adds bounded headless SNI/DBusMenu behavior and a lazy bar menu.
Private-bus proofs cover duplicate/custom-path items, menu/icon updates, owner
loss, watcher takeover and session reconnect. Nested CPU input covers native
menu open/close, both wheel axes, middle/primary activation and idle retirement.
The shared dismissal-buffer configure failure found there is fixed. See
[B0016](specs/0016-system-tray.md) for validation and the remaining boundaries.
Core enablement needs published daemon hosting/action contracts; rows stay
hidden with a shared daemon client. UI PSS+DRM+SwapPss is unmeasured, so the
memory gate remains open. Current Nacre QML has no tray reference to compare.
No live session, deployment or push was performed.

## 2026-10-22 — B0003 measured stage

Five alternating clock-only/stage-enabled pairs per compositor and one valid
200-key stage diagnostic per compositor were captured with the frozen B2 UI and
real `menu.query` helper. All 10 pairs passed per-key trace, quiet, geometry,
scope, memory and cleanup gates. Stage observation added about 0.6ms UI CPU/key;
the first typed key remained the cold outlier. No product patch or target
acceptance is claimed. B0003 remains in Review because exact changed-item IDs,
query identity and RPC RTT are not present in the published markers. See the
[measured report](research/b0003-measured-20261022.md).

## 2026-10-22 — B0002 desktop relative gate passed

Desktop Hyprland and niri both passed the owner-relative gate. Candidate-minus-control medians were +0.121/+0.134/+0.219/+0.047 MiB for Hyprland and +0.237/+0.249/+0.229/−0.998 MiB for niri; anonymous deltas stayed below +1 MiB. DRM was unknown and excluded. Candidate 306474f0 / UI 7948c3c. See [the batch report](research/b0002-desktop-batch-20261021.md).

## 2026-10-20 — B0002 Hyprland capture stopped at combined-legacy

A1 passed all four settled gates after a strict postcheck schema correction; its raw evidence is unchanged. B1 passed the first three settled phases but stopped at the required combined-legacy endpoint: 24.541992 MiB against the then-active24.5MiB single-run stop line. The endpoint was persisted before the assertion; progress.json therefore has only three completed rows. This is a valid historical stop, not a completed A/B pair or product-regression finding. Both arms used the same frozen UI, helper, fixture tree, and lifecycle-only menu-engine route. On 2026-10-20 the owner authorized a fresh20-arm batch with explicit per-phase medians and unchanged quiet/safety/swap gates; the source patch is pending lead review and no fresh arm has started. See [the capture report](research/b0002-hyprland-capture-20261020.md).

## 2026-10-19 — B0014 accepted

The Bluetooth popout exposes a power switch only for one fresh, valid adapter.
Method v1 advertisement enables it; absent/wrong-version methods keep that sole
adapter's switch disabled, while zero/multiple/invalid/stale adapters retain the
aggregate read-only view without a switch. Topic snapshots remain the only power
readback. Requests use the shared Client/CallTicket lifecycle without replay.

Reviewed source `dcf541a9f19d37b4ef493380664ff4ff1f5a123d` integrated by normal
fast-forward to main. Exact-source full hosted
[CI39240289443](https://github.com/nacre-desktop/bead/actions/runs/39240289443)
passed: formatting, all-target Clippy, workspace tests, feature configurations,
Luau/schema, capture regressions, private lifecycle and dependency/license checks.
All six named B0014 acceptance groups passed, including real Runtime/fake-daemon
eligibility, authoritative readback, queued/dispatched retirement, fenced late
replies, boot replacement, timeout/uncertain dispatch and legacy preview. The
actual factory test also passed. Shell tests: 164 passed, 0 failed, 1 ignored.
Duplicate automatic main [CI39241346554](https://github.com/nacre-desktop/bead/actions/runs/39241346554)
on the same source was canceled by lead instruction to avoid repeating checks.
The completion receipt is documentation-only with CI skipped. No private-host,
live desktop, install, measurement, producer availability or memory/latency claim.

## 2026-10-18 — B0011 accepted

The Battery popout validates published `BatteryState`, preserves percentage and
charging as independently unknown, and caches only bounded last-good display
fields through stale/disconnected input. Its topic lease exists only while the
Runtime is open. Private Battery/Network/Bluetooth lifecycle tests require actual
Sub, fresh typed data, drained startup callbacks, Unsub and consumed late data;
the shared desktop connection stays usable. Named optional-value, invalid-data,
recovery, missing-capability and legacy tests pass. Built-in source review is
accepted; exact-source `af61d18` passed full [CI39147491406](https://github.com/nacre-desktop/bead/actions/runs/39147491406).
Validation was hosted-only: shell138passed,0failed,1ignored. No lab, capture,
installation, memory/latency or producer availability claim; unavailable until
Battery is advertised. The final receipt is documentation-only with CI skipped.

## 2026-10-18 — B0010 submitted for review

The Bluetooth popout now leases the published topic only while its Runtime is
open. It validates `BluetoothState`, follows explicit device order, filters
powered adapters and caches only the last-good display fields. Private tests
cover invalid/missing data, recovery, legacy preview, lease retirement and the
shared link. Exact-head CI `39136586458` is green at `e3e417e`; validation was
hosted-only because no local Rust tools are installed. No measurement or
producer availability is claimed; the view stays unavailable until advertised.

## 2026-10-18 — B0009 accepted

The Network popout lazily leases the shared daemon topic and displays only a
validated, explicitly mapped primary connection. Stale data retains a bounded
last-good display; Runtime retirement releases the lease without closing the
shared client. Legacy preview behavior is unchanged. Named projection and
private fake-daemon lifecycle tests pass. The test peer tolerates interleaved
keepalive traffic and stays open through the shared-link assertion. Exact-head
CI `39133884809` is green at `5d96384`; validation was hosted-only. No lab,
capture, memory or latency claim. Runtime display remains unavailable until
Nacre advertises the topic.

## 2026-10-18 — B0008 submitted for review

Registered UI-provider overload now returns Busy/retryable/not-dispatched;
unknown or missing methods return UnknownMethod/not-retryable/not-dispatched.
The 64-call cap and no-replay behavior are unchanged. Three saturated-pool
socket tests, missing-provider coverage and B0007 regressions pass. Exact-head
CI `39127924726` at `b042e47` is green. No measurements or lab use.

## 2026-10-18 — B0008 in progress

B0007 was accepted Done at `736e778`. B0008 separates registered-method overload
from unknown methods without changing `MAX_CALLS`; work is source/tests only,
with no lab use or measurement.

## 2026-10-18 — B0007 accepted

UI-provider factory/future panics and task cancellation return one terminal
Internal error for the original request. The daemon link remains usable, and
session teardown drops task metadata without replay across boots. Four private
socket tests pass; exact-head CI39125066926 is green. No measurement or live
work occurred.

## 2026-10-18 — B0005 review submitted

Acceptance timing and hosted regression tests are published at `bb98474`; CI
`39079511622` is green. Seven fake-clock tests and syntax checks pass. No capture
ran and no measurement is granted. B0002 still lacks source-proven artifacts,
the `0192` ruling and an explicit slot grant.

## 2026-10-18 — B0002 artifact/preflight submitted

The current B1/B4 production runtime inputs are byte-identical. Available old
bundles are from earlier code, harness-dirty and incomplete; no source-proven
B0002 binaries or helper pairing are ready. Five pairs with full 60s phase-idle
fit about 94–124 minutes; 600s warmups raise this to about 4h54–5h24, so request
separate three-hour compositor windows if needed. No build or capture ran.
`0192` owner-policy and measurement grants remain blockers.

## 2026-10-18 — B0003 source-only preparation

The typed query path and clock boundaries are mapped in
[the source report](research/feature9-gate-d.md).
UI active-view work is the best-supported CPU candidate; transport, row decode
and compositor tails remain unresolved. No builds, product edits or measurements.
Capture requires B0001/B0002 completion and a separate exact-hash/window grant.
B0001 publication remains blocked on the owner ruling for inherited RUSTSEC-2026-0192.

## 2026-10-18 — B0004 dashboard review coverage fixes

B0004 closes ready-state default and primary disconnect evidence gaps from
B0001. Exact fixtures are checked before projection; all four page defaults and
primary-source stale retention pass, including Today's clock/store path. The
41-row inventory remains exact. CI39074426606 is green on `56210a6`; no runtime,
fixture, PNG golden or measurement changed. B0001 and B0004 are submitted for
spec review.

## 2026-10-17 — B0001 core dashboard completion

B0001 adds the five named deterministic dashboard proofs and a 41-file QML
coverage inventory at Nacre `5f4c0bf3`. Tests cover page defaults, four-tab and
legacy routing, unknown/stale source behavior, reachable clipped rows, and page
Runtime retirement. Retirement restores fake-source `Arc` counts and stops
callbacks while providers remain alive; Session state and CPU scene drop with
Runtime. GPU/raster memory is not measured. No page behavior or golden changed.
Media/player discovery, GPU metrics and reserved dashboard topics lack runtime
providers; unsupported optional cards remain deferred. No measurements run.
Validation: workspace tests 241 passed / 0 failed (3 ignored), trace memory tests
1 passed (1 ignored), fmt, warning-denied Clippy, service boundary, CPU-only and
all-feature checks, minimal facade, generated Luau (52 modules), UI check,
private service-owner proof and workspace/framework cargo-deny all pass.
[CI39072111811](https://github.com/nacre-desktop/bead/actions/runs/39072111811)
is green.

Coverage inventory: [dashboard-core-coverage.md](research/dashboard-core-coverage.md).

## 2026-10-17 — Scoped zbus advisory exceptions green

Nacre protocol remains pinned to published 5f4c0bf3, fully green in
[Source CI39052564401](https://github.com/nacre-desktop/nacre/actions/runs/39052564401).
Bead commit 3512dd7 adds only RUSTSEC-2026-0370/0371/0372 to both cargo-deny
configs. These informational unmaintained notices concern crates merged into
zbus 6; no vulnerability is reported. AccessKit, AT-SPI and tray still require
zbus 5. The comment links upstream and requires review when those consumers
support zbus 6 or by 2027-01-31. Workspace and eight framework cargo-deny checks
pass locally; [CI39069240648](https://github.com/nacre-desktop/bead/actions/runs/39069240648)
is green. The existing ttf-parser exception and all other deny rules remain.

Seven exact compatibility transcripts replay through Bead's Mirror; two new
consumer tests cover 7 snapshots / 29 patches. Published Luau checks pass 100
positive cases, 7 negative guards and 32 semantic assertions; original 30
fixtures stay byte-exact. Reserved contracts are not runtime capabilities.
No production consumer, UI, backend or helper behavior is changed.

236 workspace tests, trace, fmt/all-feature Clippy, CPU-only/release, service
boundary and 52 Luau modules pass. Both full native20 proofs pass: settled
H 23.756 / N 24.258 MiB, max closed 23.760 / 24.289; all 76 close/settled samples
fit own-bar + 5 MiB / 25 MiB and settled < 24.5. SwapPss 0; process audit 0.
File/anonymous/shared H 15.859/7.203/0.693, N 15.973/7.180/1.105 MiB.
Decoration SHM stays 542,592 B; executable +38,728 B (text +15,104 B).
Single fresh nested run/compositor with frozen75157 helper, no own build/test
overlap. Differences from earlier runs are not a paired regression or speed
claim. DRM/refresh remain unknown. [Report](research/feature9-gate-d.md#green-compatibility-protocol-pin--2026-10-17).
Published-head CI: [Actions](https://github.com/nacre-desktop/bead/actions).

## Four core dashboard tabs and stop point

Today/calendar, Weather, enrichment-only Media, and System with Updates have
lazy presentations, navigation and legacy tab/drawer routing. Page bundles,
state, subscriptions and rasters release after closed + idle. Concrete per-page
decoding stays in place. Eight PNG goldens are unchanged by this pin update.
Explicit labeled fixtures supply preview data; unadvertised real topics remain
unavailable. No MPRIS/player, backend or extra capability is invented.
The earlier five alternating pairs pass never/once/repeated-open phases;
repeated settled medians H 22.098 → 22.742 / N 22.672 → 23.264 MiB.
[Core-page report](research/feature9-gate-d.md#four-core-dashboard-tabs--measured-continuation-2026-10-17).

Advisory policy is resolved. Dashboard B0001 is in progress under its published
spec; later lifecycle/memory and typing work still require their own grants.
Optional tabs remain incomplete and unclaimed.
Optional tabs, customization/QML behavior and real provider hosting are not
claimed complete. QML stays daily driver; no later feature, live test, install,
config/unit/keybinding change or desktop job/slot. Private preview remains
prepared with immutable artifacts and explicit fixtures, not run live.
Revised compatibility/OSD schemas are adopted; Reader trim API is now published
but its application/cache gap stays open. Runtime hosting follows Nacre F4;
helper.execute is the actual compatibility method, with no guessed RPC.

Keep typing targets, CPU regression 29.61 → 32.66%, 6b pacing/idle, long chaos,
remaining caches, matched QML dashboard comparison and binary-name clash open.
No brush optimization ships: the prior exact-pixel screen had mixed CPU direction.
CPU small views / GL large animation keeps the ~20 MiB Mesa-residue policy.
Core forecast stays 40–81 eight-hour agent-days (320–648 task hours), practical
3–8 calendar weeks at 16–20 task hours/day availability plus contract/hardware
gates. Full-core idle 90–150 MiB is an estimate. Further IME/reader work and live
validation remain deferred/held; the 25 MiB ceiling / 24.5 MiB stop are unchanged.
