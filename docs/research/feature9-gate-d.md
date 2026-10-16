# Features 9–10: foundation integration

Owner authorized notifications/OSD, then the core launcher, stopping at Gate D.
No live-session test, notification-name takeover, installation or release is
allowed. The earlier single physical Gate C authorization was consumed.
Nacre and Orient remain read-only except additive nacre-protocol work through
Nacre CONTRIBUTING. Owner resumed Features9–10 after Foundation F5.
Gate D review is ready with typing/shared-engine comparison gaps; acceptance is the owner's decision.



## Gate E — active focused block, 2026-10-16

Owner approved typing/RPC, shared UI contracts, static-text AccessKit and a
prepared-only live input/hardware plan, then a Gate E decision. QML remains the
daily driver; no Feature11, deployment or live test is authorized. The stop report
will define agent-days in hours, translate full parity into calendar time, and
cost a core switch separately from on-demand QML Settings/Colors/Welcome/AI/software.
English-only core use defers IME/screen-reader validation, not basic keyboard input.

### Same-host persistent query diagnostic

Regular published75157fd3 helper, same320 Alpha/Application fixture as typing,
100 warmups followed by2,000 alternating a/ap calls per compositor on one link.
Each completed ID/query/revision and101 ordered rows is checked. No keyboard,
UI, compilation, catalog edits or per-call CLI process runs during the capture.
High-resolution whole-process clocks bracket the full loop; provider population
must remain identical. RTT includes Python JSON decode/validation; CPU excludes
that client. This is a component diagnostic, not keystroke-to-presentation latency.

|Persistent RPC|H virtual120 /N nested60|
| --- | ---: |
|Daemon CPU ms/query|3.993 /4.433|
|Provider CPU ms/query|1.572 /1.636|
|RTT p50 /p99 ms|6.218 /11.238;6.550 /11.526|
|Reply bytes|25,630–26,233|

This corrects the unmatched fresh-CLI diagnostic: handshake work was included
there. The gap versus migration's other-host full-wire~1ms still exists on a
persistent link, so neither connection setup nor keyboard churn alone explains it.
Host/provider environment and the wider typing window remain confounders. The
previous100-result decoder diagnostic~0.17ms is too small to explain the whole
gap; no unsafe decoder or alternate encoding is introduced. Migration received
the method and results plus the delta-row/stale-query questions. General daemon
notification hosting and the final combined-service idle baseline remain upstream
gaps; the official F3 QML adapter has not landed. Raw private artifacts:
`gate-e-persistent-component-v2/`. The initial upstream lab attempt failed before
measurement because its wlr-randr dependency was absent; existing Bead isolation
was reused instead, without installing a dependency or changing its proof guards.

## Gate D review — 2026-10-16

Recommendation: adjust. The framework has a useful memory advantage in these
component tests, but the typing targets and several core integration contracts
remain open. Keep the working daily QML shell while these are resolved. This is
an owner review point, not acceptance of full parity or authorization for Feature11.

All new captures are fresh private/nested sessions at2880×1800,scale1.5:
Hyprland virtual120Hz and niri nested60Hz. Feedback refresh0 means unknown,
not physical120Hz. No own build overlaps captures; shared-host contention is
uncontrolled. CPU is process CPU across threads; it can exceed elapsed latency.
Native UI timing starts at key receipt and joins the exact completed typed reply,
commit and presentation. No pending/echo/error frame substitutes for a result.

### Current launcher versus unchanged QML

The UI-only test feeds both views the same320-app canonical fixtures, with160
changed result IDs on a/ap edits. It performs no filtering/ranking/provider work.
The unchanged NacreAppBrowser retains its normal details/control/delegate lifetime;
Bead has a simpler virtualized list. These differences are part of this measured
component comparison, not a claim of full visual parity or a renderer-only effect.
QML source is unchanged between the reference checkout and Nacre75157fd3.
One200-key capture per side/compositor, then60seconds idle; no significance claim.

|UI-only component|Bead H /N|QML H /N|
| --- | ---: | ---: |
|CPU mean ms/key|9.171 /9.040|113.911 /116.460|
|Presented p99 ms|16.311 /21.933|115.939 /102.074|
|Typed-list PSS+DRM MiB,60s|37.448 /42.336|574.955 /555.438|
|Observed idle CPU %,60s|0 /0|0.550 /0.533|

Observed0 means no process ticks within10ms resolution, not zero work. QML
includes actual DRM80.906/49.773MiB; CPU Bead has no DRM client. No SwapPss
observed. Helper CPU is unmeasured in this fixture, never substituted with0.
Raw `gate-d-final-comparisons/fixture/`; existing QML presentation probe reused,
source-identical, no instrumentation rebuild. The shared-engine production QML
comparison remains missing until the migration agent's official F3 adapter.

### Real daemon path and typing budget

Regular Nacre75157fd3 helper (CI38892933475 green) is identical on both sides.
The catalog contains320 synthetic desktop entries; initial grid320, typed a/ap
100 apps plus one action. Catalog invalidation to321 entries refreshes without a
key. This one-pair native repeat compares the bounded renderer/control to the
current client host using the same current UI bundle. It is functional/performance
evidence, not a five-pair acceptance of a new host optimization.

|Real engine,200 keys|Control H /N|Current H /N|
| --- | ---: | ---: |
|UI CPU mean ms/key|11.293 /11.328|10.045 /9.860|
|Presented p99 ms|22.571 /31.083|25.425 /35.507|
|Daemon CPU mean ms/key|4.855 /5.135|4.973 /4.870|
|Observed provider CPU mean ms/key|1.934 /2.051|2.013 /2.091|

UI8ms and presentation16.7ms budgets fail. The tail increases in these single
pairs are not accepted latency improvements. Daemon/provider clocks bracket
injection through observed presentation, a wider window than UI receipt→present;
background work is included. Do not add these CPU totals as serial elapsed time.
H raw `client-native-signals-checks/typing/`; corrected N raw
`client-native-final-checks/typing/`. Earlier small-window N captures and a
shutdown-failing lifecycle remain failed/diagnostic evidence, not replacements.

A separate32-key stage diagnostic finds CPU medians H/N: markup3.827/2.613ms,
view IR1.151/0.791,layout0.695/0.430,bake0.970/0.630,composition1.433/2.081.
Markup includes setup1.035/0.729,eval1.461/0.941 and serde1.073/0.813.
Nested spans overlap and must not be added as independent work. Text shaping
is small in this warmed fixture (shape0.024/0.016ms). menu_query wall medians
9.357/7.729ms include RPC work/wait; its requester CPU is0.572/0.451ms.
The12-call native CLI diagnostic before keyboard/UI activity, excluding the
first call, measures daemon/provider CPU medians H5.045/1.818,N5.039/1.826ms.
Thus this host's cost is not explained solely by per-key keyboard-device churn.
It is not a measurement isolating transport from engine work. The migration
agent received the method/results; its desktop API numbers use another host.
Raw `client-native-final-checks/profile/` and `gate-d-final-comparisons/api/`.

### Real panel lifecycle and memory

Three cycles per compositor use the real shared menu/UI-provider connection.
Every UI close/hover/settled guard stays within its own bar+5MiB; the combined
UI+daemon/worker group also passes after35s idle. All six warm cache reports
have no `cpu-repair-scratch` allocation: real idle release is verified.
Private compositor/bus infrastructure is excluded from this product group.

|PSS+DRM+SwapPss MiB|H|N|
| --- | ---: | ---: |
|UI bar /third warm /settled|22.334 /47.240 /21.965|23.417 /45.271 /22.854|
|UI+daemon/workers bar /warm /settled|55.195 /98.006 /55.047|56.232 /96.243 /55.858|

The current warm group is about7/9MiB below Feature6's roughly105MiB figure,
but the component and driver/service populations differ; this is context, not a
matched saving. Current warm daemon/providers account for about51MiB; roughly33
remains in the daemon at rest. UI-only and whole-foundation costs stay separate.
The accepted bounded repair's first-open deltas+0.209/+0.120MiB and five-pair
warm/closed spreads are recorded below. No memory waiver was used.

### Notification/OSD comparison

Current expanded notice and volume75OSD use identical visual fixtures and the
same permissive SVG. Each side/compositor has one60-second idle capture. Native
uses a bounded fake Source; neither side owns a notification server in this UI
comparison. Bead retains its normal private service/watchdog retry cost. Functional
private-bus replacement/actions/expiry/history/DND/inhibition/hover and all four
OSDs remain covered separately, including the new daemon-routed counts/clear demo.

|Populated component|Bead H /N MiB|QML H /N MiB|Bead idle CPU H /N %|
| --- | ---: | ---: | ---: |
|Expanded notice|20.720 /20.922|140.052 /129.480|0.183 /0.150|
|Volume OSD|22.282 /23.359|91.997 /106.835|0.183 /0.217|

QML records0 ticks at10ms resolution in all four cells. Bead's nonzero idle
work is included. Totals are PSS+actual DRM; CPU Bead has no DRM client. No
SwapPss observed. Presentation latency for these matched static components is
unmeasured. Regular versus semibold summary, richer image hints/history browser,
swipe/row-height polish and160ms stack fade versus QML230ms row transition remain
visual gaps. Raw `gate-d-notice-comparisons/`; screenshots show populated content.

### Shipped view/transport changes and limits

Menu results come exclusively from the typed Source. Conversion values, errors
and provider guidance are projected from protocol v1; three populated headless
goldens and a clearing/replacement test cover them. No calculator/parser/catalog,
ranking or provider was implemented in Bead. Engine basic arithmetic, conversion
copy tokens and non-app activation remain upstream/shared-contract gaps. Copy is
not enabled without an engine-issued item/token; descriptive text is not a token.

The bar can register `ui.dispatch` and share that SDK connection with its menu
Source. Audited legacy arguments/capabilities still pass through the existing
peer-checked IPC path; optional notification controls target the private notice
view. No always-on forwarding process or per-key command process was added.
The generic provider callback is tested against bounds/private-envelope bypass.
`ui.menu.activate` remains explicitly unsupported pending the shared contract;
forwarding into the same worker that is awaiting an activation RPC would deadlock.
The migration agent has that constraint. Reconnect/peer/boot/resync transcripts
remain covered; the delayed reconnecting UI mark is still open.

A real shutdown bug was caught by lifecycle, not waived: early nacre-link/Tokio
threads had SIGINT/SIGTERM unblocked while the UI used signalfd. SIGTERM could
terminate the whole bar (exit−15) before `STOP surfaces=0`. A scoped signal mask
around thread creation now gives every thread the block and restores the caller.
An inheritance/restoration test plus strict H/N preview/lifecycle exit0+STOP pass.
All165tests/fmt/clippy warnings-denied/all-targets/all-features/CPU-only/release pass.
Static text is currently absent from AccessKit; interactive input/button checks
pass. This is a remaining accessibility gap, not silently claimed covered.

The small lazy-strip-mask experiment f651c04 is rejected and local-only. Five
memory pairs give warm/closed increases H0.436/0.432 versus control ranges
0.389/0.342MiB; N0.384/0.442 versus0.239/0.291. H CPU9.125→9.201ms/key is
neutral/slower; N typing stopped after one pair. All close+5 guards passed, but
that does not waive warm/closed regressions. The accepted renderer was restored
byte-for-byte; its exact rounded-pixel fix remains. Raw `lazy-mask-fivepairs/`.

### Owner preview and stop point

The repository command is in README: `python3 scripts/try-bead.py --daemon
/path/to/nacre-helper --bead /path/to/nacre-shell` (one shell line). A task-local
ready-to-run wrapper is supplied in the handoff. It builds/installs nothing and
starts a niri window, private daemon, HOME/XDG stores and buses, bar/launcher and
notification view. No input injection or live notification ownership. Close the
window or use Ctrl+C in its launching terminal. Strict headless H/N smoke and a
private compositor-disconnect test verify shutdown and no owned descendants.
A stale socket file after compositor exit is detected by a private connect probe.

Launcher queries and notification UI controls use the daemon; notification
content/history hosting is still Bead's transitional private service. General
daemon notification hosting is not implemented upstream. This limitation is
stated by the preview and must not be mistaken for daemon notification parity.
Nacre/Orient/installed state remain unchanged. Backend adapters stay frozen except
fixes; no shell move, binary-name resolution, installation or Feature11 is started.

The forecast below remains roughly99–201 focused agent-days, with uncertainty
concentrated in managed windows, media/video, AI, input and hardware. Recommended
next order: typing/RPC and shared-engine QML/actions/reconnect; input/hardware
proofs; shared service panels/dashboard; Settings/Colors/Welcome/session; managed
windows/pearls, static wallpaper then video, AI rich views, software/theme/extension/
dev/AUR surfaces. Keep hyprlock. Owner decides whether to fund that continuation.

## Bounded rounded repair — 2026-10-16

Owner resumed a roughly two-hour experiment; the accepted solution took under
one hour including full checks and five alternating pairs per compositor.
Repair-sized RGBA/mask scratch includes a corner/stroke guard, reuses capacity,
reserves at most16MiB in the process budget and releases after500ms quiet or
surface destruction. Full repairs reuse the existing target/mask. Conservative
opaque-interior/covered-prefix/bounds shortcuts remain exact. Uniform masked
rounded painting also fixes a1-alpha partial/full rounding mismatch found in
the old full-canvas WIP by the expanded test. No pixel tolerance changed.

161tests/fmt/clippy all-targets/all-features denied warnings/CPU-only/release pass.
Exact corpus:6,400 repairs, four scales and fractional poses, RGBA/BGRA, clipping,
shadow, stroke and alpha; every outside-repair byte is unchanged. Separate
scratch test checks capacity reuse, cap rejection and deterministic idle release.
The initial8MiB cap failed on a nearly full1.7Mpixel repair (pixels+mask≈8.5MiB);
that aborted proof is preserved, no threshold was relaxed.16MiB is an admission
limit, not an eager allocation. All five pairs below use the corrected binary.

|Five-pair medians|Before H /N|Candidate H /N|Control range H /N|
| --- | ---: | ---: | ---: |
|First-open MiB|50.540 /48.675|50.749 /48.795|0.312 /0.186|
|Third warm MiB|48.098 /46.406|48.325 /46.475|0.292 /0.399|
|Third closed MiB|21.924 /22.812|22.007 /22.942|0.245 /0.323|
|UI CPU ms/key|9.958 /10.643|8.914 /9.883|0.634 /0.599|
|Presented p99 ms|22.083 /20.675|21.965 /25.010|7.232 /9.096|

First increases stay below1MiB; warm/closed increases fit control ranges,
including paired medians. All60 close cycles plus20 hover guards pass ownbar+5.
Second-open medians H48.102→48.369/N46.396→46.525 also fit control ranges
6.240/0.373; the H control includes a large outlier, not precision evidence.
Third-warm component medians change by file PSS H+0.169/N+0.197MiB and
anonymous H+0.027/N−0.061MiB. This is consistent with mapped-code cost, not an
allocation trace proving the absence of allocator retention. N p99 rises4.335ms inside its9.096ms control range;
paired median+2.222ms. This is not a latency win or proof of no pacing issue.
CPU paired medians−0.855/−0.657ms;4000 exact receipt→present key proofs pass.
Both8ms/16.7ms targets remain unaccepted. Hvirtual120/Nnested60, actual feedback
refresh0 means unknown. Original per-key wtype, clock-only profiling, fresh
private sessions, no own builds; host contention uncontrolled. Raw immutable
artifacts/patches and `bounded-scratch-fivepairs/{memory,typing}-summary.json`
are preserved locally. Old7271a86 and both failed compact prototypes stay local;
only the bounded, budget-passing renderer is integrated.

## Native Luau value constructors — 2026-10-16

The SDK builds alpha/tint brushes, binding markers and action records in Luau.
It preserves the existing numeric/string coercions, shared action arguments,
nil versus false, caller diagnostics, fresh sandbox,8MiB limit and50ms deadline.
Both API contract tests pass before/after; no bind resolution or action execution
moves into Luau.159 tests/fmt/clippy ALL targets/CPU-only/release pass.

Five alternating200-key fixture pairs each use identical per-key injection and
clock-only flags. CPU medians H11.243→10.727/N11.309→10.695ms/key;
paired medians−0.976/−1.268, control ranges2.382/0.824. Presented p99 medians
22.514→22.360/24.149→21.145ms, no tail regression.4000 exact key proofs pass.
These are paired-run wins, not a comparison against an earlier quieter run set
or a claim that either typing target is accepted. Private Hvirtual120/N60,
scale1.5, unknown refresh predictor; no own build overlap, shared-host contention
uncontrolled. Raw `luau-value-helpers-fivepairs/summary.json`.

Five lifecycle pairs each: Hfirst49.586→49.624,warm47.414→47.456,
closed21.080→21.200MiB;N47.732→47.745/45.558→45.569/21.974→21.950.
H warm/closed deltas0.042/0.120 fit control ranges0.347/0.183; N0.012/−0.023
fit0.221/0.200. First-open is effectively unchanged and below the1MiB rule.
All60 cycle plus hover guards pass ownbar+5. No memory waiver or new retained
cache; compiled SDK bytes remain charged to the existing markup budget.
Raw `luau-value-helpers-lifecycle-{finish5,fivepairs-summary.json}`; pair1 in
the typing directory. One separate native A/B pair each on the same published9374bc8d helper gives
UI CPU H11.898→11.363/N12.109→11.728ms/key; p9927.854→28.930/
31.270→30.094ms. This is a functional repeat and a limited CPU check, not
five-pair native tail acceptance: H tail is higher in this single pair. Targets
remain open. Raw `luau-values-native-pair-v2/`. V1 completed200 keys but failed
cleanup on a worker exiting between enumeration and /proc read; finally cleanup
passed. V1 remains failed evidence, v2 tolerates only expected process-exit races
and still verifies no owned live worker remains.

## Diagnostic exclusions — 2026-10-16

The headless `menu-codec-profile` example uses golden-derived100/320-item replies,
64-byte opaque descriptors and ordinary display fields. Three1000-iteration
trials on one thread measure process CPU including destruction. Full decode
averages0.170/0.919ms for24,014/76,594bytes; raw JSON parse0.063/0.255,
DOM clone0.041/0.154, clone→typed Message0.158/0.690. Validation is below0.001ms,
not measured zero. Raw JSON is only a diagnostic lower bound: no duplicate-key,
size or other protocol check is removed. No alternate decoder is built on this
small100-result cost. Raw `codec-profile-current/profile.json`.

A16-key per-compositor same-binary wire-traced diagnostic compares per-key
wtype with one persistent fixed250ms keyboard. H10.832→11.833ms/key,
N13.372→13.288; no consistent CPU benefit. H has16 keymap events after input,
Nnone; batch hasnone. Different gaps and wire tracing are confounders. It is not
an optimization or physical validation. The experimental batch driver is dropped,
its exact harness source preserved privately; default injection stays unchanged.
Raw `keyboard-device-diagnostic/`, not mixed with budget measurements.

Owned thread accounting is an optional external harness diagnostic, not production
thread tracing. It checks thread start times and never counts disappeared threads as
zero. Over200 keys on9374bc8d, all threads remain accounted: full-trial CPU
H14.20/N13.20ms/key versus exact input→feedback H12.535/N12.860. Two UI
threads contribute H7.75/5.15,N7.65/4.10; link thread1.30/1.45. Whole-trial
figures include gaps and keyboard management, with0.05ms/key tick resolution,
and cannot replace the exact-window budget. Raw `native-thread-diagnostic/`.

Migration confirms F3 is already authorized, not a new approval gate. It will
finish remaining F2, then the shared QML adapter and F4; no duplicate Bead client.
CLDR/base catalogs are embedded, no separate local staging is missing. Basic
arithmetic remains an engine completeness gap before F3, not a client math-parser
request. Published9374bc8d /CI38883651768 is green; the private query-cache audit
candidate is not yet published or accepted here. Cold first query~18.3ms on the
migration host remains open even after warm-cache improvements. Host/provider
populations differ; do not substitute those numbers for this host's measurements.

## Rejected static-property borrowing — 2026-10-16

Copy-on-write borrowing avoided cloning RawNode properties without bindings.
All157 tests/fmt/clippy ALL targets/CPU-only/release and pixel goldens passed,
but five alternating200-key typing pairs each did not show a robust CPU benefit.
H CPU median9.980→9.888ms/key (paired−0.244,control width0.682);
N9.881→10.664 (paired+0.288,width1.527). Presented p99 medians22.282→22.246 /
19.694→20.938ms, inside control widths5.853/6.413.4000 exact key proofs pass.
A separate lifecycle pair passes all guards, but cannot rescue the CPU result.
The source patch is saved and reversed; no property borrowing is kept. This does
not identify why total CPU changed; do not claim a proven cache/locality cause.
Raw `borrow-properties-fivepairs/summary.json` and
`borrow-static-properties-rejected-source.patch`.

The independent catalog-selection fix is retained: if an unchanged query loses
its selected result, fallback selection and scroll reset together. Valid restored
selection/scroll stays intact. Its regression test uses a Source revision change
without editing the query. No ranking/provider change or new retained cache.
The control binary includes this fix; the rejected candidate adds only borrowing.
A brush-helper contract test records existing values/numeric coercion/diagnostics
before a separate measured experiment. All current typing targets stay open.

## Real-engine Source and refresh checkpoint — 2026-10-16

Nacre engineb44d1afb /CI38874838563 is green. Bead pins its protocol and replays
all five menu transcripts, including conversion/guidance. The launcher observes
menu topic changes. A generic Session bug skipped external projection on empty
Source wakeups; the no-change check now follows projection. A regression test
proves no-op, staged refresh, atomic commit and last-good failure preservation.
Only idempotent menu.query busy+retry=true replies retry,25ms doubling to400ms,
inside the original3s absolute deadline; activation never retries. Tests cover
flag/code/deadline limits.156 tests/fmt/clippy ALL targets/CPU-only/release pass.

Three fresh200-key runs per compositor,320 synthetic apps,101 populated results
for a/ap:1200 exact RPC ID→typed revision/count→committed/presented update proofs.
No busy retry during timed keys. Every run also presents a321-entry catalog
invalidation without a key and stops its owned workers.32s daemon settling
retires startup workers; the first typed key still starts a cold emoji worker.
Native CPU medians of per-run means and presentation p99 medians:

| |Hyprland|niri|
| --- | ---: | ---: |
|UI CPU ms/key|12.230|12.047|
|Daemon CPU ms/key-window|8.332|8.101|
|Provider CPU ms/key-window|1.944|1.856|
|Presented p99 ms|31.559|30.360|

Neither8ms UI nor16.7ms presentation target is accepted. CPU is high-resolution
process time: UI input receipt→exact feedback callback; daemon/workers bracket
injection→observed feedback, including background work. These are not pure RPC
costs or directly comparable to upstream's different-host/send→reply benchmark.
UI CPU run ranges0.406/0.472ms. Cold-first values remain separate in the raw
summary. Actual nested backend selection and private socket guards are explicit.
Configured Hvirtual120/N60,scale1.5; feedback refresh prediction unknown, not
physical validation. No own build overlaps; shared-host contention uncontrolled.
Raw `engine-typing-three-runs-v2/{manifest,summary}.json`.

Fixture memory A/B: H10 pairs (one fixed five-pair confirmation of a17KiB
borderline warm result),N5 pairs. First/warm/closed MiB medians:
H50.545→50.770/48.205→48.291/21.838→21.994;
N48.663→49.088/46.206→46.658/22.782→23.124.
H warm/closed deltas0.086/0.156 fit control ranges0.747/0.789;
N0.452/0.342 fit0.605/0.594. First-open increases stay below1MiB. All90cycle
checks plus hover guards pass ownbar+5; original five-pair H result is retained,
not replaced. This establishes no delta outside the measured ranges, not exactzero.
Raw `engine-adapter-lifecycle-{deadline-fivepairs,confirm5}/` and ten-H summary.

One real-engine lifecycle each compositor adds external process observations to
the existing three-open/hover/query/no-durable-query proof. Warm UI+daemon/workers
H47.745+49.217=96.962MiB/N45.609+50.237=95.846MiB. After35s idle,
UI+external H22.205+30.980=53.185/N23.052+32.063=55.115;
bar baselines21.492+30.128=51.620/23.246+31.252=54.498. Both combined
settled groups and every UI close pass matchedbar+5 including SwapPss. External
DRM devices are checked; absent counters are accepted only with no DRM device
open. Provider process retirement explains the warm→idle reduction. This is not
a matched memory comparison against Feature6's different~105MiB sound workload.
Raw `engine-lifecycle-candidate/`; compositor/private-bus infrastructure excluded.

Separate16-key detailed diagnostic after startup-only sequential CLI queries:
H/N daemon observed0ticks over2s (quantized, not exactzero). Later11 broad a/ap
API calls average daemon6.991/10.073ms,worker1.958/1.926. High cost therefore
persists without repeated keyboard-device injection or a long presentation
window. Detailed markup CPU3.575/3.734ms,IR0.980/1.101,layout0.592/0.633,
bake1.347/1.390,compose2.442/3.070; thread menu-query wall11.184/12.615 versus
CPU0.522/0.558 excludes other UI threads' CPU. Inclusive spans are not additive;
this diagnostic warms the provider and adds tracing, so it is not a before/after
optimization result. Raw `engine-component-diagnostic/`.

Preserved failures v1–v7: wrong pointer environment/interrupted cleanup fixed;
empty Source wakeup fixed; provider busy with a concurrent diagnostic query;
explicit-backend refresh still busy without it;175ms retry exhaustion after200
valid typing updates. All cleanup now runs even on a helper assertion. The final
3s deadline retry passes the unchanged catalog proof. Migration confirmed
repeated filesystem events queue overlapping metadata loads and is coalescing
them, reserving interactive capacity. No helper code is edited in Bead. Engine
SHA58014d70…b18d remains immutable; repeat the native proof after the new build.
Matched production QML on this engine remains unavailable until migration F3;
there is no attach/serve forwarding and no parallel client substitutes. Optional
conversion/guidance visuals and UI-provider integration remain open. No Gate D
acceptance, physical/live test, install or release is claimed.

## Rejected duplicate bake serialization/copies — 2026-10-16

A prototype hashed only pose after the existing local primitive signature and
prepared each raster without cloning unrelated scene contents. Pixel/clip/pose/
raster-reuse tests, fmt, all-target clippy,153 workspace tests, CPU-only check and
release build passed. It is nevertheless rejected: the CPU win did not repeat.

Five alternating clock-only typing pairs each,200 keys/cell:

|Per-run medians|Before H /N|Candidate H /N|
| --- | ---: | ---: |
|UI CPU ms/key|9.719 /10.200|10.030 /10.146|
|Presented p99 ms|22.466 /18.560|21.896 /21.561|

H paired CPU+0.513ms; N−0.059ms, marginal gain inside control width0.446.
N p99+3.001ms is inside the3.465ms control range, but CPU still lacks a robust
benefit on both compositors. One separate lifecycle pair reduced first/warm/
closed memory on both; that does not rescue a candidate without its CPU win.
All4000 exact key proofs and lifecycle guards pass. The source patch and immutable
binary remain private for audit; the patch is reversed, absent from kept code.
Raw `bake-copy-fivepairs/summary.json`, `bake-copy-pair/`,
`bake-copy-rejected-source.patch`. Same private Hvirtual120/N60,scale1.5,
unknown refresh prediction; no own builds during capture, external contention
uncontrolled. A static-property borrow prototype is unbuilt and separate.

## Launcher input parity — 2026-10-16

Result selection and navigation are view state over the engine's unchanged order.
A successful query edit selects the first enabled result and resets scrolling;
the empty apps grid retains its unselected initial state. Arrows use actual
rendered column/row/viewport geometry, skip disabled rows and keep selection in
view. Enter uses the opaque engine descriptor; rejected activation stays visible.
No catalog, ranking, provider or real external action is added. Every early
scene-return path now runs common asset/font/editor preparation before commit.

153 tests/fmt/clippy ALL targets/CPU-only/release pass. New tests cover query
reset versus restored state, disabled rows, list wrap/scroll and grid boundaries.
The existing backend reconnect fixture now waits for its event socket to be
accepted before disconnecting it; production backend code is frozen. Private
nested input proofs pass on both compositors:13 states each,12 exact generation/
update presentations plus one acknowledged unchanged boundary action. The
no-change action correctly does not demand a new frame. A fixture activation
error is presented, keeps selection and leaves the client alive.

Five alternating lifecycle pairs per compositor use the constructor checkpoint
as control and fresh private sessions. PSS+DRM MiB medians:

| |Before H /N|Input candidate H /N|Control range width H /N|
| --- | ---: | ---: | ---: |
|First open|50.748 /48.464|51.082 /48.792|1.260 /1.145|
|Third warm|48.171 /46.146|48.567 /46.367|1.467 /0.991|
|Third closed|21.923 /22.582|22.187 /22.768|1.482 /0.953|

All60 cycle checks plus hover/idle guards pass bar+5MiB. Positive memory deltas
are inside the control ranges, not evidence of an exact zero delta. Paired closed
medians are+0.044/+0.128MiB. No confirmed closed/steady regression is resolved by
waiving a threshold. First-open increases remain below1MiB. Raw evidence:
`.research/gate-d-foundation/navigation-lifecycle-{threepairs,finish5}/` and
`navigation-lifecycle-fivepairs-summary.json`.

Typing checks show mixed changes within the recent run spread, not an
optimization: reverse-order H clocks9.188→9.444ms/key, p9922.204→21.900;
N10.129→10.847,p9923.038→17.047. The earlier order has H9.531→10.826,
N10.522→10.355. Raw `navigation-final-pair/`, `navigation-reverse-profile/` and
`launcher-input-navigation-v6/`. CPU8ms/p99≤16.7ms targets remain unaccepted.
Configured nested Hvirtual120/N60,scale1.5; presentation refresh prediction
unknown. No own build overlaps; external load, frequency and paging uncontrolled.
SDK-only optlevel2 was separately rejected: mixed tails/resident SHM lifecycle;
SDK level1 remains. An unmeasured bake-copy experiment is excluded here.

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

## Changed-result typing and active-view preparation — 2026-10-16

The preceding identical-row a/ap fixture is a best-case model-reuse workload,
not evidence that newly filtered results meet the typing target. An additional
explicit fixture keeps320 total apps, but a/ap return disjoint 160-row sets.
Every edit replaces visible result identities; no filtering/ranking/provider runs.
Both UIs receive the same rows/order/icons/breadcrumbs and200 exact-generation
key→query commit→presentation proofs. Screenshots show Application0160 and ap.

The Luau launcher constructed both AppGrid and Results before hiding one. A new
model therefore rebuilt the hidden grid too. Selecting the active branch before
construction removes that work without retaining another prepared view. Existing
store-owned query/selection/scroll survive; headless coverage switches disjoint
results, provider fixtures, reload and back to the full320-app grid.

|Nested changed-result fixture | Before H /N | Active view H /N | QML H /N |
| --- | ---: | ---: | ---: |
|UI CPU mean ms/key |36.568 /35.595|23.897 /26.059|133.780 /135.558|
|Presented p99 ms |48.053 /50.516|38.966 /40.492|110.741 /111.242|
|Typed-list PSS+DRM MiB,3s idle |43.72 /47.50|42.42 /46.44|502.09 /488.93|

Hyprland virtual120/niri nested60,2880×1800/scale1.5. Same release binary
30804a7 (SHA-256 dbaee209120c8e0bc087d546b3f190e3fc9e484e53a88c51cf34950df245738a),
only candidate Luau bundle differs. Original unchanged QML browser retains its
extra details/controls and normal delegate/model lifecycle; this is no claim of
full visual parity or an engine-cost comparison. QML initially missed an explicit
reference-path environment setting; that failed setup was rejected and corrected.
Native H→QML H→QML N→native N→candidate H/N, sequential fresh private sessions;
no own heavy build overlap. Migration compilation was paused, but its desktop
comparison/diagnostic and other activity were uncontrolled. One cell per side,
not an alternating five-pair significance proof. No SwapPss observed. Three-second
memory samples are diagnostics, not replacements for60-second idle acceptance.

H steady-profile CPU: markup16.22→5.95ms; worker23.55→10.57ms, composition
11.85→12.42ms. Nested spans overlap and must not be added as independent work.
This is a real preparation improvement, but both8ms CPU/16.7ms presentation
budgets still fail for changed results. Composition and remaining markup need
work; helper CPU/end-to-end remain unmeasured until the canonical engine lands.
The docs-only5a7f548 CI repeated the50ms markup failure after e2ba01d had passed;
the active-view candidate must pass a fresh full CI run without raising that limit.
Local135tests/fmt/all-features clippy/Luau pass. The default launcher lifecycle
proof passes both: hover expiry, explicit preparation, preserved query, three
closes within bar+5MiB including swap, exact feedback and process cleanup.

## Tight text rasters — 2026-10-16

Caching a raster did not make its composition cheap: text images retained their
whole layout width, including transparent padding. Numeric paint spans identified
text blending as the dominant cost. Newly baked text now stores only nonzero-alpha
ink plus an integer pixel origin. CPU and native GL add that origin after rounding
the original position; damage tracks the resulting ink bounds. Layout, shaping,
fonts and hit targets remain unchanged. Empty text retains one transparent pixel.

|200 changed-result keys, UI only | Before H /N | Cropped H /N |
| --- | ---: | ---: |
|UI CPU mean ms/key |24.946 /25.691|18.454 /18.774|
|Presented p99 ms |33.525 /41.354|30.426 /29.015|
|Typed-list PSS+DRM MiB,3s idle |42.517 /46.243|38.821 /42.459|
|Steady text composition CPU ms/key |6.799 /7.808|0.560 /0.617|
|Steady whole composition CPU ms/key |12.314 /13.990|6.229 /6.550|

Both binaries use the active-view bundle and identical numeric spans. Before is
59f00f1 plus diagnostic spans; candidate adds cropped text. Same 320-app/disjoint
160-row a/ap fixture,200 exact receipt/commit/presentation generations. H virtual120,
N nested60,2880×1800/scale1.5; sequential beforeH/candidateH/candidateN/beforeN,
fresh private sessions. No own heavy build overlap; external activity/frequency
uncontrolled, migration's independent diagnostic may overlap. One pair per side,
not a five-pair invariance claim. No SwapPss;3s memory is diagnostic only.

Composition and retained bytes improve, but8ms CPU/16.7ms presentation targets
still fail. Remaining markup serializes the complete model into Luau to obtain
its length; that is a separate next experiment. Helper/end-to-end remain unmeasured.
136 tests/fmt/all-features clippy/release pass, including unchanged notification/OSD
goldens and a new exact-pixel check for both formats, fractional scale/translation,
clips, very low alpha, partial repaints and moving cropped origins. Counter tests
still show no new layout or rasterization during transform/opacity replay.
Matched default launcher lifecycle controls/candidates pass both, including swap:
H bar/first/third warm/third closed MiB22.056/52.733/51.196/22.173 →
22.312/51.166/48.424/21.906; N23.358/51.084/50.212/24.124 →
23.715/49.369/46.503/19.401. All three candidate closes H21.70–21.91 and
N19.40–22.79MiB stay within their bar+5. This sequential comparison shows no
first-open/steady/closed regression; it is not strict idle invariance. Native GL
smokes pass on both with grid↔list input, exact feedback, actual GL initialization
and cleanup; centered grid text was inspected. Hyprland debug overlays remain in
the short GL capture; its timing is not an acceptance measurement.

## Count-only model access — earlier candidate held, 2026-10-16

A small native model_count(binding) accessor avoids serializing every result into
Luau solely to calculate VirtualGrid's extent. Repeat still validates every key
and renders only its window.137 tests/fmt/clippy/release/Luau freshness pass,
including count-only dependency tracking through growth, empty and invalid models.
The candidate is preserved as local 95862ca, not integrated or published.

One 200-key pair each, followed by two alternating 80-key pairs per compositor,
uses identical cropped binaries/bundles except the count API. Same 320-app/disjoint
160-row fixture, fresh private sessions; own builds stopped, external load and
frequency uncontrolled. CPU evaluation becomes cheaper in every trial.

|Trial means, ms UI CPU/key | Before trials | Candidate trials | Median before→candidate |
| --- | --- | --- | --- |
|Hyprland |17.215,18.398,19.082|18.008,15.647,16.083|18.398→16.083|
|niri |20.326,20.818,18.217|17.830,16.267,16.740|20.326→16.740|

Presented p99 is not uniformly improved: H trials31.61/38.67/32.40→
24.82/30.66/30.32ms; N27.40/29.47/30.07→26.44/36.70/38.10ms.
The shorter samples have different tail resolution. Their phase/load effects
were not isolated; these measurements do not establish a causal pacing regression
or accept either typing budget. Do not hide the mixed tails behind CPU medians.

All before/candidate lifecycle checks pass bar+5 including swap, hover expiry,
query preservation, three closes and cleanup. H bar/first/third warm/closed MiB
21.729/51.095/48.323/21.782→22.257/50.945/48.381/21.958; N
23.314/49.120/46.562/22.825→23.768/49.005/46.607/22.887.
The small positive steady/closed totals need confirmation before integration.
H closed+0.176MiB includes+0.164 file-backed and+0.012 anonymous; the candidate
ELF is3,072 bytes larger. This is not proof of an equivalent retained-heap leak,
nor permission to waive the closed-memory rule. No new tolerance was introduced.
The default development executable was restored to the accepted cropped build;
all candidate sources, binaries, logs and provenance remain available locally.

Canonical menu schemas/engine are still unavailable. Upstream F2 is held for a
separate regression/priority review; the owner decision blocks end-to-end work.
The additive menu draft remains off main with its migration owner. Stop at this
dependency decision; Gate D is not accepted and no Feature 11 work has started.

## Count-only decision: five alternating pairs — 2026-10-16

The owner resumed independent typing work and requested five alternating pairs per
compositor. Each binary has200 changed-result keys and a separate three-close
lifecycle proof in fresh private sessions. All40 proof invocations pass:4000 keys
and80 closed/unused-hover checks. Same saved cropped/count binaries and fixture;
no local heavy builds overlap. H runs before/candidate five times, N reverses that
order. External load/frequency/paging were uncontrolled, so these are shared-host
nested measurements, not an isolated or physical120Hz acceptance.

|Five trial medians | Before H /N | Count-only H /N | Baseline spread H /N |
| --- | ---: | ---: | ---: |
|UI CPU mean ms/key |17.459 /19.973|16.160 /17.775|3.175 /2.583|
|Presented p99 ms |30.296 /29.822|25.129 /27.198|7.243 /3.957|
|Third warm PSS+DRM MiB |48.059 /46.397|48.238 /46.432|0.391 /0.586|
|Third closed PSS+DRM MiB |21.544 /22.638|21.788 /22.692|0.547 /0.638|

Spread means baseline min-to-max of per-run metrics, not pooled keys or an inflated
candidate band. Both marginal and paired CPU medians improve (paired−1.830/−1.446
ms). Both p99 medians improve; candidate maximum p99 excess on N is only0.161ms,
inside its3.957ms baseline spread. Positive median closed deltas+.244/+.055MiB
and paired deltas+.290/−.003MiB fit the baseline spread. Steady warm deltas
+.180/+.034MiB also fit. First-warm medians change−.006/+.033MiB, below1MiB.
Every individual close still passes bar+5 including swap; no tolerance was changed.
One candidate first-warm H sample is unusually low; its cause was not established.

Keep95862ca's count-only API under the owner's stated criteria. This supersedes
the earlier hold, not the recorded mixed short-trial results. The complete numeric
cohorts/provenance remain in the ignored five-pair artifacts.137 workspace tests,
fmt/all-features clippy and generated Luau checks pass; source matches the tested
saved candidate. Both8ms CPU and16.7ms presentation targets still fail. Next is
ordered pixel-damage reuse, measured independently, and canonical protocol adoption.

Nacre's reviewed additive menu contract is published as3042f3f2; source CI is
checked after publication. Its eleven protocol tests/export/fmt/clippy pass and
all nine original wire artifacts remain byte-identical. No helper/registry change
was made. The real engine follows the time-boxed startup investigation before F2
finishes; end-to-end UI/daemon CPU and Gate D remain unaccepted until measured.

## Canonical menu Source — 2026-10-16

Bead pins Nacre3042f3f2 and consumes its typed menu.query/menu.activate payloads.
Fixture replies now use stable-ID items plus authoritative order. Validation
rejects obsolete row-array replies, invalid ordering and oversized fields. The
projection sends only display data into Luau; opaque activation descriptors and
catalog_rev stay in the host. Activation echoes those fields to the engine and
honors keep_open plus optional query/mode/opaque-route view updates. Sidebar
navigation clears the engine route; bounded route/query/selection remain RAM-only.
No catalog, ranking, providers or external action execution was added to Bead.

Four committed menu transcripts match the pinned exports and replay calls/errors
over a fake Unix daemon, retaining peer/path checks and the existing reconnect/
boot-id suite. JSON object key order is compared semantically, as in the original
wire tests; entity order arrays and request/action fields remain exact. Deadlines
must count down within the requested budget. The fake server waits for the actual
welcome handshake; no disconnected call is treated as success.

141 isolated workspace tests, fmt/all-features clippy, release/CPU-only check,
23-module generated Luau checks, service boundary and all independent permissive
framework license/source graphs pass. Private native H/N smokes each present12
changed-result edits; the unchanged QML component receives the same canonical
items through a thin display adapter and presents6 edits each. These short runs
verify compatibility, not typing acceptance or matched performance.

|Canonical launcher lifecycle smoke, MiB | H | N |
| --- | ---: | ---: |
|Bar |22.115|23.541|
|First warm /third warm |51.100 /48.810|49.091 /46.801|
|Third closed |22.132|22.916|

Every close/hover expiry remains within its own bar+5 including swap. Query RAM,
geometry, feedback and cleanup pass. Samples are fresh private Hvirtual120/N60,
not physical or strict idle invariance. No local heavy build overlaps captures;
other load/paging is uncontrolled. Real engine CPU and end-to-end remain missing.
Nacre's full3042 CI hit the same pre-existing helper/Pillow parity failure as its
parent; migration owns that fix (83a9a9a4), and its current CI is tracked separately.
The protocol-only eleven tests/export pass; no helper code/goldens were changed here.

Renderer ID/difference experiments remain separate local work, not included in
this Source checkpoint or its141-test count. Gate D is still pending the real
engine/provider/input comparison and updated measured forecast.

## Plain constructor property copies — 2026-10-16

Keep the native Luau table-clone path for ordinary property tables. Validation
still checks every key; caller tables are not modified. Protected/metatable
properties retain the manual path, and child extraction uses raw accesses with
the existing sparse-sequence behavior. Tests cover plain snapshots, frozen and
protected tables, sparse children and the existing caller file/line diagnostics.
No VM reuse, cache, language restriction or font change is part of this step.

|5 alternating typing pairs, per-run medians |Before H /N |Candidate H /N |
| --- | ---: | ---: |
|UI CPU mean ms/key |10.464 /10.012|9.658 /10.061|
|Presented p99 ms |22.383 /19.278|22.075 /18.479|

H CPU gain−0.805ms (paired−0.926). N CPU is unresolved/neutral: marginal+0.049,
paired−0.027, baseline range1.033ms. Do not claim a measured N CPU gain. P99
medians improve−0.308/−0.799ms within baseline widths2.237/3.346. Both budgets
remain unaccepted. This retains a clear H improvement without a resolved N
regression.4000 exact clock-only key proofs pass.

Separate three-cycle lifecycle controls/candidate each: first/third/closed H
50.791/48.707/22.282→50.537/48.324/21.973MiB; N48.927/46.754/23.096→
48.802/46.606/23.062. All strict bar+5/query/hover/geometry/cleanup guards pass.
151 tests/fmt/clippy ALL targets/CPU-only/release pass. Controlab4329d2;
candidate8f238209. Fresh private configured Hvirtual120/N60, actual scale1.5,
unknown feedback refresh prediction; external contention uncontrolled, no own
build overlap. No physical/engine acceptance. Later selection/navigation drafts
are excluded. Raw: `.research/gate-d-foundation/constructor-typing-fivepairs/`
and `constructor-clone-pair/`.

## Borrow existing font state for caret metrics — 2026-10-16

Keep font-system reuse after five alternating typing pairs and five lifecycle
pairs per compositor. The baseline loads fonts once per key for caret metrics,
although the bake renderer already holds the same explicit font set. A bounded
weak index shares that renderer-owned state; it does not retain another pool.
Fresh-system output/teardown/stamp-invalidation tests pass. Native shaping,
font selection, scale and sandbox behavior remain unchanged.

|Per-run medians,5 pairs each |Before H /N |Candidate H /N |
| --- | ---: | ---: |
|UI CPU mean ms/key |10.638 /11.311|10.095 /11.137|
|Presented p99 ms |22.271 /20.732|21.903 /19.742|
|First warm MiB |50.557 /48.733|50.396 /48.671|
|Third warm MiB |48.297 /46.571|48.285 /46.401|
|Third closed MiB |22.029 /22.964|21.943 /22.863|

Paired CPU median differences−0.543/−0.649ms; marginal differences−0.543/−0.174.
N's marginal CPU gain is small versus its1.641ms baseline range: report the
variation, not a large speed claim. P99 median differences−0.368/−0.989ms fit
baseline ranges0.804/5.930. Per-key font-load records1→0 in both compositor probes.
Initial three-pair H memory was borderline; completing five yields lower first/
steady/closed medians. All closure/hover/query/geometry/cleanup guards pass, no
threshold changed. Known index bytes1304; dependency font internals remain opaque
and use the existing renderer teardown. No new strong font ownership or thread.

The checkpoint also validates that a menu reply belongs to the requested query,
keeping the last good view on mismatch. Optional numeric `MENU_RPC_SENT/RETURN`
request IDs/revision/count and typed `MENU_RESULT` markers prepare the real-engine
readiness chain without logging query text or rows. Fake-daemon/transcript tests
continue working. This is readiness instrumentation, not an end-to-end engine proof.

150 tests/fmt/clippy ALL targets with warnings denied/CPU-only/release pass.
Control27ec2fec/UI147; candidateab4329d2. Exact fixtures,320-app catalog,200 keys
per typing run, clock-only;3-cycle separate memory cells. Fresh private Hvirtual120/
N60 configured, scale1.5; no own build overlap, external load/frequency/paging
uncontrolled. Refresh predictions unknown. No physical acceptance; both typing
targets and real-engine CPU remain open. Later constructor-copy experiment excluded.
Raw: `.research/gate-d-foundation/font-typing-fivepairs/`,
`font-lifecycle-threepairs/`, `font-lifecycle-finish5/`.

## Clocked UI typing improvements — 2026-10-16

Keep the UI candidate after a matched clock-only comparison. Ordered paint records
avoid repainting identical pixels just because action/state IDs change. Cached
raster deltas refresh their comparison base and cover shrinking/growing extents;
removed tails are cleared, GPU uploads clip to current texture bounds. State IDs
remain unchanged in the view/state layer. Luau constructors retain the same
property registry, caller diagnostics, fresh sandbox and50ms/8MiB limits. Projected
rows share immutable storage; selection uses copy-on-write. Collapsed selections
shape the caret boundary once.

SHM previously unioned distant dirty regions into one bounding box before drawing.
Each reusable buffer now keeps at most8 repair regions, including all missed-frame
history. CPU draws them separately; non-CPU SHM fallbacks retain bounding draws.
The existing one-pixel presentation request for pixel-identical new generations
stays intact. Exact three-buffer repair tests cover RGBA/BGRA, scale1/1.5 and alpha.
Diagnosis: four repair rectangles process3.4% of their bounding-box area; H detailed
composition6.56→3.75ms before this step, then component3.62→1.87ms with separate
regions. The unbuilt translucent shortcut was shelved: fixture fills are opaque.

|Clock-only per-run medians,5 alternating pairs |Before H /N |Candidate H /N |
| --- | ---: | ---: |
|UI-process CPU mean ms/key |15.467 /16.957|10.099 /10.830|
|Presented p99 ms |24.715 /27.019|22.191 /21.338|
|First warm MiB,10 lifecycle pairs |50.796 /48.296|50.224 /47.623|
|Third warm MiB,10 pairs |48.510 /46.197|47.949 /45.604|
|Third closed MiB,10 pairs |21.867 /22.292|21.638 /22.026|

CPU median differences−5.368/−6.127ms; paired median−5.885/−6.581. Positive p99
regression is absent in the medians; baseline p99 range widths6.550/2.804ms.
The first five H closed trials were borderline (+0.105MiB vs baseline width0.088).
Five further alternating lifecycle pairs each resolve it without a waiver:
combined closed deltas−0.229/−0.266, baseline widths0.832/1.007; paired deltas
−0.216/−0.225. First/third warm medians also decrease. All current closed and
unused-hover guards pass bar+5MiB including swap; teardown/query RAM/geometry pass.

One original N pair04 candidate state command returned nonzero. Its original raw
cell is retained under `launcher-failed-ipc01`; the exact diagnostic passed and is
labelled in `retry-review.json`, followed by20 clean confirmation sessions. The
original reply was not captured, no panic marker was found, and its cause remains
unclassified. The harness now saves failing command reply/exit/client status;
no deadline, presentation, memory or lifecycle assertion was relaxed.

147 local tests/fmt/clippy/CPU-only/release and native GL6-key smokes each pass. Control is measurement-only
e4145f5 (binary953a03e5); candidate27ec2fec has the measured UI code, not later
font/RPC drafts. Fresh private sessions,200 keys then a separate3-cycle lifecycle
cell per trial; no own heavy build overlap, external load/frequency/paging not
controlled. Buffers logical1164×654/physical1746×981,scale1.5,RGBA on both.
Hvirtual120/Nnested60 are configured; both feedback refresh predictions are unknown.
No physical or real-engine acceptance. Helper CPU is missing, not zero. Both typing
budgets stay open. Raw: `.research/gate-d-foundation/typing-clock-fivepairs/` and
`clock-lifecycle-confirm/`; full data includes the failed cell and diagnostic retry.

## Typing clocks and tracing — 2026-10-16

`BEAD_TYPING_PROFILE=clocks` keeps whole-process CPU and monotonic input→exact
commit→presentation timestamps, suppressing optional per-stage spans. Default1
keeps detailed profiling for diagnosis. The proof requires positive clock progress,
exact update feedback, clock ID1 and no stage spans in clocks-only mode. Unknown
presentation refresh predictions are null, not a measured zero-Hz rate.

Calibration used the same unaccepted147-test UI binary in eight fresh private
sessions,64 changed-result keys each: H detail/clocks/clocks/detail, N reverse.
No own heavy build overlap; shared-host load/frequency/paging uncontrolled.

|Per-run UI CPU mean ms/key | Detailed values | Clocks-only values | Median difference |
| --- | --- | --- | ---: |
|H configured virtual120 |12.935,11.262|8.797,11.576|−1.912|
|N configured nested60 |13.168,13.340|11.805,11.701|−1.501|

All512 exact key proofs pass; each64-key detailed run emits12832 stage records,
clocks-only emits none. H spread is large; do not call the mode difference an
optimization or accept either typing budget. Compare future before/candidate with
matching clocks-only instrumentation and retain detailed profiles separately.
Both buffers are logical1164×654 /physical1746×981 at scale1.5, RGBA. Both provide
valid monotonic presentation timestamps but report unknown refresh prediction.
The configured rates above are not physical or feedback-verified refresh rates.
[Protocol definition](https://gitlab.freedesktop.org/wayland/wayland-protocols/-/blob/main/stable/presentation-time/presentation-time.xml).
Raw method/manifest: `.research/gate-d-foundation/clock-mode-calibration-v2/`.

## Rejected ID-map damage experiments — 2026-10-16

A same-paint-position fallback for completely replaced primitive IDs reduced
composition cost, but its closed/steady memory did not meet the standing rule.
Five alternating H lifecycle pairs after the canonical Source used fresh private
sessions, three openings and the unchanged bar+5/hover/query/geometry proofs.
The third-closed median rose21.875→22.176MiB, exceeding baseline range width0.208;
third-warm48.557→48.846 exceeded width0.239. Anonymous closed memory fell slightly;
per-mapping smaps attributed roughly0.37MiB to private executable-backed pages.
No driver/allocator/cache workaround or memory tolerance was added.

A smaller two-pass map implementation also failed the memory comparison despite
CPU wins: H18.172→12.760/N18.504→15.329ms/key, presented p9931.303→23.854/
27.099→23.913ms. Third-closed H21.601→22.229/N22.695→23.255MiB; third-warm
48.329→48.907/46.537→47.069. Both implementations are rejected, saved locally
for audit, and absent from accepted results. Nested Hvirtual120/N60;200 changed-
result keys per cell, no own heavy build overlap; shared-host load is uncontrolled.
A simpler ordered-record vector is measured separately before any decision.

## Remaining-parity forecast — Gate D review

Gate D is not accepted by this forecast. UI-only matched comparisons are attached;
the production QML/shared-engine and full prefix/activation comparison remain open. Estimates are focused
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
| Remaining bar/popouts and service views |6–12|Panel registry/lifetimes, audio devices/mic, network/Bluetooth/tray/power/calendar; real service errors and multi-output dismissal|
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

Total roughly99–201 focused agent-days. Calendar assumptions and the narrower
core-switch alternative are below. There is
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
QML rewrite. Stop for the owner's Gate D decision before any Feature 11 work.

Open carry-forwards: historical typing29.61→32.66% workload, first-key/overall N
latency,6b0a nested/shared-GL pacing and strict idle invariance; physical120Hz
launcher/IME/screen-reader/NVIDIA validation; non-app UI-provider activation/reconnect mark
integration; static-text accessibility; dependency-internal font caches bounded by teardown rather than exact
byte reports; richer notice/OSD polish; large-catalog paging beyond current virtual
extent limits; binary/control-script name collision. The permissive framework/GPL
product boundary and CPU-small-panel/GL-large-animation policy remain. Mesa's
roughly20MiB post-EGL residue remains a reason to measure GL lifecycle, not to
unmap driver memory or silently waive the closed bar+5MiB condition.

## Rounded-repair memory stop — 2026-10-16

A new exact property test fails against the accepted renderer: at scale1,RGBA,
zero offset,damage(24,22,12,8),pixel(30,22),partial[50,58,110,255] differs from
full[52,57,111,255]. The antialiased rounded curve is clipped differently when
translated into a sub-strip. Full-canvas geometry with an output repair mask
passes the complete new corpus: scale1/1.5,fractional offsets,RGBA/BGRA,
clipped/corner/translucent content, and unchanged bytes outside damage.
No tolerance is widened. The published SDK changes do not alter this renderer.

The correctness control791a50…2007 and conservative fast candidate250c01…9548
pass160 tests/fmt/all-target warnings-denied clippy/CPU-only/release. The candidate
also rejects rounded bounds outside damage, uses exact constant interiors and
starts at the last fully covering opaque fill. The test asserts both coverage
and corner fallback. No new retained raster/cache or layout/raster animation work.

One complete200-key typing/lifecycle A/B per compositor compares the fast correct
candidate against accepted SDK230dc7…6715, not just the slower correct control:
CPU H9.887→9.536/N9.581→9.514ms/key,p9922.128→22.381/21.999→18.567.
These short CPU results are not five-pair acceptance. Memory is disqualifying:

|MiB|Before H /N|Candidate H /N|Delta H /N|
| --- | ---: | ---: | ---: |
|First open|50.909 /48.638|51.885 /50.109|+0.976 /+1.471|
|Third warm|48.515 /46.538|49.639 /47.769|+1.124 /+1.231|
|Third closed|22.310 /22.950|22.121 /23.075|−0.189 /+0.125|

The continuation completed a second H pair before interruption:
first50.869→52.064,warm48.521→49.741,closed22.298→22.353. Its first-open
increase also exceeds1MiB. These are preserved short samples, not noise estimates.
N SHM is unchanged at7.455MiB; first anonymous24.875→26.105 and warm
23.008→24.000. H first anonymous+1.055/warm+1.168. The full-canvas mask and
allocator retention are the likely cause; no allocation trace proves exclusivity.

The owner's rule requires stopping for first-open>1MiB or steady/closed regression.
No exception is inferred from all close/lifecycle guards still passing ownbar+5.
Further runs were interrupted in the exact owned tmux session. A leftover private
parent compositor was identified by its exact runtime in retained logs, stopped,
and only its generated runtime removed. No daily desktop process was touched.
Accepted code and shared release binary were restored; no bad renderer published.
Local WIP branch wip/gate-d-rounded-aa-memory-stop-20261016,commit
7271a8605f1cb93e41d746b03136fd5c0c738455 preserves the full fix/test/shortcuts.
Raw `rounded-correct-fast-pair/`, interrupted`rounded-correct-fast-finish5/`,
`rounded-memory-stop-checkpoint.json`,all immutable binaries/source patches remain.

Recommended next direction is a bounded scratch representation that preserves
full curve geometry without a full-surface mask allocation; it is unimplemented
and needs new exact-pixel,CPU and memory proof. The alternative is to leave the
published behavior and retain this pixel gap. Owner decision is requested under
the explicit stop rule. Gate D is not reached; no Feature11 or live test begins.

### Calendar interpretation for the Gate E decision

One agent-day means **8 hours of focused task elapsed time**, including analysis,
implementation, failed experiments, local tests and CI supervision. It is neither
24 hours nor a promise of eight hours of code output. The old20–40 worker-week
translation assumed five8-hour days; it does not describe this chat's long runs.
Recent dated checkpoints span most of the day, including overnight work. Git
cadence is evidence of sustained activity, not a time sheet; focused hours have
not been instrumented. Use **16–20 task hours per calendar day** as an explicit
planning scenario for the current mostly-all-day schedule, not a measured daily
average or a promise that all waiting time produces progress.

Full parity99–201 agent-days is792–1,608 task hours: roughly40–101 active calendar
days at that schedule, or6–15 weeks. Allowing shared-contract and supervised
hardware scheduling gives a practical planning window of about7–18 calendar
weeks. Owner pauses and unavailable hardware can extend it; those waits are not
hidden implementation hours. Migration runs in parallel, but one cannot simply
divide the total by the number of chats. This remains a forecast of hard unbuilt
work, not an extrapolation from quick bar checkpoints. Re-estimate at Gate E.

### Core switch alternative — estimate only, not implementation approval

Bead would own the always-on bar, popouts/service panels, launcher, notifications
and OSD, dashboard, static wallpaper and hyprlock/session integration. The current
QML Settings, Colors, Welcome, AI and software-library windows would start only
when opened, use the same daemon and exit completely when closed. Bead remains
the sole desktop UI provider; on-demand windows must not replace that registration.
This needs small standalone entry points and tested window/process lifetimes,
not a second launcher engine or a permanently hidden QML shell.

|Core-switch remaining area|Agent-days,8h each|Included risk|
| --- | ---: | --- |
|Typing, pacing, reconnect and lifecycle rails|4–8|Gate E targets/candidate decision, cold key, idle/chaos|
|Bar, popouts and service panels|6–12|Audio/tray/network/Bluetooth/power/calendar; error and multi-output dismissal|
|Launcher/notification/OSD integration and polish|4–8|Shared activation/copy and same-engine comparison, bounds and transitions|
|Daemon hosting for core services|3–6|Notification ownership/history and needed bead-services; migration dependency|
|Dashboard|8–15|Data/fetch/artwork workers, weather/media and bounded idle memory|
|Static wallpaper|3–6|Decode worker retirement, one retained output image, multi-output/reload|
|Hyprlock/session integration|2–4|Existing lock, suspend/resume and failure paths|
|Basic hardware and sustained validation|4–8|English input, outputs/hotplug,120Hz, suspend and memory/wakeup slope|
|Packaging, names and on-demand QML lifecycle|4–8|Entry points, routing, close-to-exit, rollback and binary-name resolution|

Total **38–75 agent-days**,304–600 task hours: roughly15–38 active calendar days
at16–20h/day. Budget about **3–7 calendar weeks** with contract/hardware scheduling,
conditional on the migration lane and owner-supervised tests. These are additional
hours from the current checkpoint, not a request to start Feature11.

IME and screen-reader validation move to the later public-release phase for the
owner's English-only use. Basic key input, editing, repeat, clipboard, focus and
reconnect remain core requirements. Managed pearls, video/effects, Bead-native
Settings/Colors/Welcome/AI/software and public-release accessibility are excluded;
they still appear in the full-parity table. This is a personal core switch, not
full parity or a public-release claim.

**Always-on memory estimate:** current measured UI bar+daemon is55–56MiB PSS+DRM
(and SwapPss, none observed). Allow10–30MiB for remaining core service/notice
state and20–60MiB for static wallpaper: roughly **90–150MiB idle** on one
2880×1800 output. One RGBA image alone is19.8MiB; retained buffers/copies matter,
so static wallpaper is not free. Dashboard/panel warm allowance suggests roughly
120–200MiB while open, returning to the measured core baseline after close/idle.
Those additions are engineering estimates, not captured combined-service totals
or accepted budgets. Extra outputs/image dimensions can increase them. On-demand
QML memory is additional only while its window is running; close-to-exit must be
proved. No whole-shell QML baseline was captured here, so no savings percentage
is claimed. Combining notices/panels in the same UI avoids assuming another
permanent20MiB host; the existing preview's split processes are transitional.

Core blockers: owner decision on Gate E typing/presentation; production F3 shared
engine/actions and settings single-writer contracts; daemon hosting of notifications
and required audio/tray/power/etc.; reliable standalone QML entry points without
claiming the sole UI provider; wallpaper buffer lifetime; owner-approved physical
validation; packaging/name collision and rollback. The migration agent has no
measured final combined-service idle baseline yet. QML remains daily driver until
these gates are passed; this estimate authorizes no live switch or installation.
