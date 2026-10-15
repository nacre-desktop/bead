# Features 9–10: foundation integration

Owner authorized notifications/OSD, then the core launcher, stopping at Gate D.
No live-session test, notification-name takeover, installation or release is
allowed. The earlier single physical Gate C authorization was consumed.
Nacre and Orient remain read-only except additive nacre-protocol work through
Nacre CONTRIBUTING. Owner resumed Features9–10 after Foundation F5.
Typing targets are not accepted; this is an interim checkpoint, not Gate D.



## Feature 10 Source fixture checkpoint — 2026-10-16

The view consumes ordered menu.query results through the neutral Source call API.
The local XDG catalog reader and native substring filter are removed. Explicit
--menu-fixture uses exact request→reply data; unknown queries fail, and fixtures
never activate external actions. --daemon-socket selects an authenticated private
link; no ambient endpoint is discovered. Current row normalization follows QML's
result shape until the migration agent publishes canonical menu schemas/goldens.
No catalog, ranking, provider or calculator engine is implemented in Bead.

Apps use the grid; search/actions use windowed rows. Calculator/emoji/clipboard/
files/Brain-shaped fixture results exercise display and raw query forwarding;
this is not acceptance of their real providers. Selection/scroll/query are view
state. A bounded scalar RAM snapshot survives worker release; typed text never
enters the durable panel file. Search receives focus immediately on open.
The existing two-second hover/keybind preparation lifecycle hosts the launcher;
explicit bead.panel.prepare is available, unused preparation maps no surface,
and idle drops worker/rasters/SHM. Legacy nacre.launcher(query) forwards the query.
A launcher host rejects sound commands instead of opening the wrong panel.

Content-sized layout now uses output logical dimensions, not its last surface
size. The old90% maximum recursively shrank1164→1112→1065→1023… and allocated
three SHM buffers at every step. Both prepare/configure paths now stabilize at
1164×654 with three buffers per lifecycle. Default-allocator H/N three-cycle
proofs pass hover timeout, explicit preparation, RAM query restore/disk exclusion,
presentation feedback, unavailable-route checks and complete cleanup.

| PSS+DRM MiB, default CPU | H120 | N60 |
| --- | ---: | ---: |
| Bar with launcher configured |22.168|23.526|
| First /third launcher warm |53.073 /52.951|51.253 /51.051|
| First /third closed |23.559 /23.449|24.658 /24.393|
| Accepted sound guard bar /first warm /third warm /closed5s |22.073 /28.890 /26.972 /22.775|23.617 /30.924 /29.339 /23.675|
| Published-Rust control, sound bar /first /third /closed5s |22.973 /29.020 /27.596 /23.382|24.412 /31.820 /30.063 /24.407|

Every close/lifecycle remains within its own bar+5, including swap. No DRM client.
The control is the recovered default/native-gl artifact from the published F154
build (Rust unchanged in c74), with its original Cargo fingerprint and checksum
saved privately. Sequential guard samples, not five-pair strict idle invariance.
Current sound values are lower than that control; no new memory regression shown.
Historical Gate C/code revisions had different baseline file residency; do not
infer a same-revision delta from those separate runs.

A warm launcher is larger than the accepted sound panel: its three1746×981 RGBA
buffers cost19.60MiB versus4.18MiB for sound, and its full shadow raster is about
6.53 versus1.39MiB. Models, glyphs and transient allocator residency add the rest.
It remains below Feature6's roughly105MiB bar+panel figure, with different content
and renderer/code revision. This is context, not a matched optimization claim.
No allocator policy changed: child-only mmap-threshold diagnosis passed, while
tcache-off and fixed-view-buffer trials failed and were discarded. Geometry
churn explained the new failure; live heap/cache bytes alone did not predict PSS.

### Typing method and results

Private2880×1800/scale1.5 release CPU renderer,320 populated exact fixture rows,
real virtual-keyboard input, one edit in flight. CLOCK_MONOTONIC input receipt to
exact committed generation's compositor feedback; process CPU clock includes
all UI threads, excludes injector. H is virtual120Hz; niri Winit is nested60Hz.
No own heavy build overlapped; unrelated desktop activity/frequency uncontrolled.
Migration quiet was confirmed for the original80-key baseline, not every later
diagnostic. Missing helper/real-engine measurements remain unmeasured.

| Fixture workload | H UI CPU mean ms/key | N UI CPU mean ms/key | H /N presented p99 ms |
| --- | ---: | ---: | ---: |
| Before,80 keys empty/a grid↔list |54.313|54.592|66.965 /103.747|
|200 keys: first grid→list, then a/ap list |5.093|5.010|13.959 /19.057|
| Steady199 keys only |4.961|4.912|13.884 /16.485|
| First key CPU /presented |31.307 /39.059|24.572 /28.931|included above|
| All-key CPU p99 /max |6.238 /31.307|17.153 /24.572|N/A|
| Helper CPU /end-to-end engine |unmeasured|unmeasured|unmeasured|

The workloads differ:54→5 is not a paired same-workload speedup. Same40-key
steady diagnostics before/after immutable models were8.30→4.15ms H and
10.62→5.49ms N; final200-key means above confirm the lower warm cost. Structural
layout switching is still substantially slower. The first transition and old
SHM-buffer damage history cause large early frames; overall N p99 remains above
16.7ms. Targets are not accepted end-to-end or on physical120Hz scanout.

Profiling isolated~30ms composition plus~21ms worker preparation in the initial
transition case. Exact first-raster copies onto cleared damage, safe opaque
interior fills and unnecessary-mask avoidance reduce paint work. Pixel tests
cover premultiplied alpha, alternating clips, partial damage and rounded borders.
Keeping both layouts in the cached template avoids repeated Lua evaluation.
Owned reply normalization and unchanged-model deltas avoid redundant copies.
Immutable list payloads now share storage across store/candidate snapshots;
replacement remains transactional and JSON is unchanged (Rust uses Value::list).
Shared cache references use conservative byte admissions, under the same caps.

A guard-accounting bug also surfaced: the behaviour VM's50ms deadline expired
while the separate markup VM/native layout ran. The same cumulative50ms allowance
now counts active behaviour calls, including nested work, and excludes time
between calls. Tests prove native pauses do not consume it and many small calls
still exhaust it; the limit was not raised. Current workspace validation134tests,
fmt/all-features clippy/CPU-only/default release pass before final policy recheck.
Matched real QML launcher/engine comparison, real prefixes/activation contracts,
restart badge/UI-provider integration and Gate D forecast remain next work.

## Feature 9 Source UI comparison — 2026-10-15

The notification view consumes a neutral Source; the hosted private service and
bounded fake fixtures produce identical Luau bindings. Controls follow current
NacreNotice visibility: omit the implicit default action from the footer, show
expand only when details exist, retain whole-card hover for expiry pause, and
route dismiss/actions/DND/history/legacy commands through the service boundary.
The existing 256-record/3 MiB history limit remains. Daemon-owned 30-day history
and daemon notification topics are upstream work, not a second Bead store.

Unchanged NacreNotice and NacreLevelNotice visual components run with deterministic
transport shims. Bead uses a fixed Source snapshot, with no notification server;
its current transitional desktop bus/audio tasks still run against unavailable
private endpoints, and their cost is included. QML starts no such service lanes.
The notification shows the same app/summary/body/time/Open action and bundled
MIT SVG; OSD shows volume75. QML's notice container is464×350; Bead fits content.
Both show the same400-wide card and340×80 OSD at2880×1800/scale1.5.

Each cell is one60-second idle sample after3 seconds settling, fresh private
session per cell, sequential CPU-small-surface release runs. No own or migration
heavy local build overlapped; unrelated desktop activity/frequency uncontrolled.
Hyprland virtual120 Hz; niri nested Winit60 Hz. No physical timing acceptance.
Memory is process PSS plus deduplicated DRM, no SwapPss in any cell. CPU uses
/proc process tick deltas (10 ms resolution);0 ticks is below that observation
resolution, not a zero-cost claim. Raw JSON/screenshots remain ignored.

| Populated component | Bead H /N MiB | QML H /N MiB | Bead H /N CPU % |
| --- | ---: | ---: | ---: |
| Expanded notice |20.779 /21.588|161.223 /151.207|0.217 /0.200|
| Volume OSD |22.371 /23.797|125.090 /121.978|0.250 /0.217|

Bead has no DRM client; QML totals include its actual driver allocations. QML
recorded0 CPU ticks in all four cells. Bead's nonzero idle cost remains visible;
transitional service retry/watchdog work is included, not attributed by sampling.
Initial notice captures with a missing-icon placeholder were rejected; corrected
same-SVG repeats replace them. Earlier two-second smokes are not acceptance data.

Controls/OSD populated headless goldens and visibility tests pass; all123 workspace
tests, fmt/clippy/CPU+release/Luau/license boundaries pass at f154f47, CI38810904821
green. Real private-bus proofs pass on both after the Source-host refactor:
replacement/expiry/actions/inhibition, real pointer action/hover/replacement,
legacy counts/clear, history/DND restart, all four OSDs, killed-parent bus cleanup
and unchanged ambient notification ownership. Functional runs may overlap
migration builds; their two-second idle numbers are not idle CPU acceptance.
Empty/warm/closed PSS+DRM MiB H17.34/27.38/23.73, N17.65/28.57/24.31;
owned bus separately0.93/0.95 MiB. This standalone notification workload differs
from the accepted bar+sound Gate C guard. Presentation timing for matched QML
notices is unmeasured; no server-cost delta inferred from the static fixtures.

Visual limits retained for Gate D: regular-weight Bead summary versus QML's
semibold, per-row stack-height/swipe polish, rich image-hint presentation and
full history-browser styling. The implemented stack fade is160 ms; this does not
claim QML's per-row230 ms height transition. No feature beyond Gate D is started.

## Typing foundation

The new proof uses 320 apps and one edit in flight. CLOCK_MONOTONIC timestamps
join keyboard receipt to the exact validated UI commit and presentation update.
Whole-process CPU includes application/driver/accessibility threads through the
presentation callback; compositor and injector CPU are separate. This measures
application input receipt, not physical keyboard latency. Profiles separate
computed bindings, markup evaluation/serialization, layout, fonts, shaping,
assets, baking and worker metadata.

| Trial | Hypr p99 presented ms | CPU median / mean ms per key |
| --- | ---: | ---: |
| Initial instrumented grid, 40 keys, state tracing | 55.49 | 41.35 / pending matched repeat |
| Same initial grid, nested niri60 | 49.59 | 42.76 / pending matched repeat |
| Windowed grid, 40 keys | 42.48 | 31.64 / preliminary |
| Native filter, 80 keys, numeric-only tracing | 582.92 | 17.65 / preliminary |
| Font selection cache, 80 keys | 24.04 | 12.85 / preliminary |
| Text-only candidate patch, 80 keys | 17.29 | 7.32 / 7.32 |

These are development trials, not a completed paired comparison. State tracing,
host contention and sample counts differ; preserve failed/contended runs. One
startup exceeded the unchanged 50 ms script limit while another task built
niri. The native-filter latency tail also worsened during contention. Do not
claim those timing differences as optimization wins. Repeat quiet matched
baselines and final code on both compositors. Existing nested niri is60 Hz;
120 Hz physical acceptance remains unverified. CPU p99 in the latest trial was
11.12 ms, including a cold font-selection key; the target is not accepted yet.
The historical 29.61→32.66% regression remains a separate, differently paced
measurement, not a percentage convertible from the new CPU-per-key proof.

Candidates retain the complete model/scroll extent but construct viewport rows
plus overscan. Immutable markup reuse tracks its actual model reads; templates
must be deterministic over bindings. Text-only patching preserves input boxes,
placeholder color and accessibility labels; structural bindings, hover, focus,
viewport or markup changes take full validation/layout. A bounded font-path
cache avoids file rescans and resets on reload. No font data is stored there.
Generation/worker ownership releases these caches with a closed panel.

## Private notification foundation

The server starts and owns a dedicated dbus-daemon/socket in a private0700
directory. It never discovers or connects to the live session notification bus.
It implements bounded replacement/actions/expiry/history, DND, connection-owned
inhibition, hover pause and close reasons. Inhibition changes emit
PropertiesChanged; the disconnect fixture caught a stale client cache before
that signal was added. API follows the [freedesktop notification protocol](https://specifications.freedesktop.org/notification/latest/protocol.html).

Both nested proofs pass standard protocol behavior, policy ownership, history
restart, all four exact validated OSD presentations, owned-daemon cleanup and
unchanged ambient notification ownership. CPU mode has no DRM allocation.
Empty/warm process PSS MiB is15.60/27.49 Hypr,16.17/29.91 niri; owned bus
adds0.66/0.69. Idle samples are only2 s, not an idle-CPU acceptance.
Actual visual controls/hover and matched QML proofs remain in progress. The initial trial used
the old automatic GL default; it is not a CPU-small-panel memory acceptance.
Notice width/radius400/14 and bottom OSD340×80/radius22/duration150 ms follow
Nacre's current components. Native volume/mic changes and validated private OSD
commands feed the same model. Hardware integration remains nested-fixture-only.
The OSD is passive, shares the process's notification/service owner, and releases
its worker/raster/SHM state after close. Last-notice protocol closure and visual
exit lifetimes are separate; DND/inhibition suppress immediately.

Standalone notification mode currently owns the primary notice surface and one
passive OSD companion; it cannot be combined with bar/audio mode flags. Full
shell composition is not implied by this proof. Feature9 final controls,
matched QML comparison and Feature10 catalog/ranking/providers are paused for Foundation F5.

## Foundation pause checkpoint (2026-10-15)

81 workspace tests, fmt, clippy and22-module Luau analysis pass.
Private proofs now include real action clicks, replacement while hovered,
DND/history persistence, passive OSD (no keyboard enter), legacy clear/counts,
SIGKILL and same-socket restart. Both compositors pass. An inherited blocked
SIGTERM mask prevented parent-death cleanup of dbus-daemon; clearing the mask
in the child before exec fixes it without altering the parent's mask.

| Nested CPU smoke | Hyprland120 | niri60 |
| --- | ---: | ---: |
| Empty / warm / closed PSS MiB |16.01 /26.16 /22.26|16.58 /26.92 /23.23|
| DRM allocation |none|none|

Two-second idle samples do not establish steady-state idle CPU. Closed memory
includes newly loaded fonts and is not the accepted bar+sound GateC workload.
SHM pools now shrink on geometry changes and unused opacity rasters are released.
Evidence: ignored .research/gate-d/foundation-checkpoint-*; previous failures kept.
No further launcher work: nacre-daemon owns catalog/ranking/providers/menu.query.
The binary/control-script name collision is deferred to GateD as instructed.



## Matched launcher view comparison — 2026-10-16

The reference runs unchanged NacreAppBrowser and its visual dependencies, with a
thin exact-query menu.content fixture and passive service shims. No QML catalog,
ranking/provider execution or live configuration is used. Both sides receive
320 ordered app rows and the same names, icon SVG, breadcrumb/description fields
and a/ap query sequence. Bead's row subtitle now follows QML's breadcrumb fallback.
The full QML details/quick-controls pane, filter/back controls, hints and richer
input remain present; Bead currently has a core grid/list/search/sidebar subset.
This is a populated common-core comparison, not a full visual-parity claim.

Persistent idle virtual keyboards maintain seat capability on both sides. Without
one, destroying wtype's per-key virtual device made Qt lose search focus after the
first key. The unchanged-component six-key smoke proved the setup fix. No focus
reset, component rewrite or presentation assertion relaxation was used.
The injector is task-owned and stopped in finally; no live input is touched.

The optional QML probe records numeric QQuickWindow key receipt/process CPU,
queryChanged commit and wp_presentation feedback after scene synchronization.
One edit in flight; receipt/commit/presentation generation must match and clock
must be CLOCK_MONOTONIC. First key included, then a/ap;200 keys on each side.
Native uses the same input/presentation endpoints and process CPU clocks. All
threads count, injector CPU does not. Async QML images retain normal behavior;
readiness/count and screenshots were verified, not substituted for presentation.
No own build overlaps these sequential fresh-session captures; H native→QML,
N QML→native, unrelated activity/frequency uncontrolled. H virtual120/N nested60,
2880×1800/scale1.5. Helper CPU/end-to-end remain unmeasured on both fixture views.

|200 keys, fixture UI | Bead H /N | QML H /N |
| --- | ---: | ---: |
|UI CPU mean ms/key |5.270 /5.723|5.689 /5.548|
|Presented p99 ms |15.091 /15.828|15.743 /12.697|
|Steady presented p99, excluding first |14.289 /10.280|9.760 /12.499|
|Warm typed-list PSS+DRM MiB, after60s |37.804 /41.812|253.516 /208.511|
|Observed idle CPU %,60s |0 /0|0.467 /0.467|

Observed0 means no process ticks within10ms tick resolution; no zero-cost claim.
QML includes actual DRM81.12/49.83MiB; Bead has no DRM client. No SwapPss observed.
Idle behavior differs: QML retains normal focused text-input/extra controls;
Bead's input polish is limited, so do not attribute the idle difference solely
to renderer or engine. The results show a large memory difference and similar
warm CPU, with compositor-dependent tails. They do not accept physical120Hz,
real prefixes/actions/ranking, changed-result workloads or full Nacre parity.
Cold first transitions remain separately recorded and can exceed one frame.

Initial broken fixtures (missing singleton/formatting fields, duplicate signal,
missing marker log level, transient keyboard focus) were rejected. Final logs have
no runtime ReferenceError/TypeError, exact200-generation presentation proofs pass,
and screenshots show populated rows and query ap. Corrected native captures
replace the earlier differing breadcrumb-label trial. Raw artifacts remain ignored.
Validation:135 workspace tests, fmt and all-features clippy pass; C++ probe
build, private smoke/200-key proofs and unchanged-mode reload guard pass. The
reload guard retains properties, keeps the prior UI on syntax error and recovers.
Predecessor684b922 CI38823402035 green.
Canonical daemon menu schemas/engine are still pending after upstream F2; no
second engine was built to turn missing measurements into zeros.

## Remaining-parity forecast — provisional until the engine comparison

Gate D is not accepted by this forecast. The engine/prefix/activation comparison
and final matched QML measurements must be attached first. Estimates are focused
agent-days including implementation, fixtures, nested validation and CI, assuming
stable daemon contracts. They are not calendar promises or an extrapolation of
bar throughput. Recent small-panel, foundation and Source checkpoints took hours
to about a day each; the first launcher exposed a geometry loop, allocation
residency and multi-stage typing cost that several simple components did not.

Read-only QML inventory has10 popout files,41 dashboard components including
Today/Focus/Media/System/Weather,40 Settings components and the managed pearl
window. File counts are scope pointers, not equal units of work. Services owned
by the migration agent are dependencies, not a second implementation budget.

| Remaining area | Focused agent-days | Hard unproved part |
| --- | ---: | --- |
| Finish engine/typing/pacing/idle rails and shell integration |4–8|Changed-result queries, cold first key, N tail, reconnect badge/UI provider, three-hour lifecycle/chaos|
| Remaining bar/popouts and service views |6–12|Audio devices/mic, network/Bluetooth/tray/power/calendar; real service errors and multi-output dismissal|
| Dashboard pages |8–15|MPRIS/artwork/resource/calendar/weather, bounded decode/fetch workers, event/state consistency|
| Settings/Colors/Welcome/control/session windows |12–24|Schema-driven edits across65 stores, revision errors, forms, shortcuts, previews and restart recovery|
| Pearls/managed windows |10–20|xdg-toplevel lifetime, detach/dock, geometry/focus/output restoration and compositor placement limits|
| Static wallpaper/video/effects |10–22|libmpv GPL boundary, decode/GPU copies, frame synchronization, transparency/effects and multi-output cost|
| AI/sidebar/Brain rich views |15–30|Streaming markdown/code/selection, attachments, cancellation, credentials/capabilities and large histories|
| Input/IME/accessibility |10–20|Composition/candidate windows, complex scripts/font fallback, selections, clipboard and real screen-reader flow|
| Software/themes/extensions/dev environments/AUR review |15–30|Catalog/detail/install/review/progress/error flows, root operations and existing permission policy parity|
| Hyprlock integration and session policy |2–4|Keep hyprlock; daemon handoff, failure/resume behavior, no native lock implementation|
| Hardware/long-duration validation |5–12|Physical120Hz/mixed outputs/hotplug/suspend, NVIDIA/driver variants, idle wakeups/energy and memory slopes|
| Repository/deployment/name integration |2–4|Owner-approved shell move, pinned dependencies, control-script/binary name clash, rollback and packaging|

Total roughly99–201 focused agent-days (about20–40 single-worker weeks), with
large uncertainty concentrated in pearls, media, AI, input and hardware. Parallel
work can shorten independent views; shared daemon/window/input dependencies and
owner-supervised hardware gates prevent simply dividing the total by agent count.
The physical/hardware portion also depends on access and owner availability.

Recommended order: close the engine/typing and reconnect/idle rails; validate
input and basic hardware before multiplying pages; complete shared service panels
and dashboard; then Settings/Colors/Welcome/session views. Prove managed windows
before pearls/AI-heavy workflows. Add static wallpaper before video/effects, then
AI rich views, then the software/theme/extension/dev/AUR surfaces. Integrate
hyprlock when session policy needs it. Keep the working QML daily shell while
these gates remain open; move touched features to the daemon instead of a blanket
QML rewrite. Stop for the owner's Gate D decision before any Feature11 work.

Open carry-forwards: historical typing29.61→32.66% workload, first-key/overall N
latency,6b0a nested/shared-GL pacing and strict idle invariance; physical120Hz
launcher/IME/screen-reader/NVIDIA validation; daemon UI-provider/reconnect mark
integration; dependency-internal font caches bounded by teardown rather than exact
byte reports; richer notice/OSD polish; large-catalog paging beyond current virtual
extent limits; binary/control-script name collision. The permissive framework/GPL
product boundary and CPU-small-panel/GL-large-animation policy remain. Mesa's
roughly20MiB post-EGL residue remains a reason to measure GL lifecycle, not to
unmap driver memory or silently waive the closed bar+5MiB condition.
