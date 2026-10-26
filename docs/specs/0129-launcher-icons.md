# B0129 — launcher icons on the first frame

The launcher must open with every visible app icon already rasterised, including
after its 120-second panel-worker expiry. Path lookup caching alone cannot keep
decoded pixels alive across that expiry. Startup warming must happen after the
bar presents, and catalog updates must refresh likely icons without keeping a
second catalog or view alive.

Budget: prefer **1 MiB** additional always-on unreclaimable memory; this unit
permits up to **4 MiB** only with a measured smoothness benefit and an explanation.
The general raster LRU is capped at 1.5 MiB including conservative entry charges;
resident worker/allocator overhead must be measured separately. Clean file PSS is
informational. Compare CPU renderer, identical full-release flags, same private
output geometry, at least five alternating control/candidate pairs.

Acceptance: no missing-icon frames for cold, warm, and expired opens; first-frame
open latency no more than control +2 ms; first-key UI CPU at most 8 ms. A bounded
initial-data wait must prevent committing an empty intermediate model. A failed
icon gets a fallback; an unavailable service must not block opening indefinitely.
Overview, palette, picker thumbnails, widgets, and drag latency are out of scope.

The renderer shares a small-image raster LRU across panel workers, keyed by
immutable contents, physical dimensions, fit and path. It admits images at most
128×128 physical pixels, with at most 256 entries and a 1.5 MiB charged-byte cap.
SVG and PNG use the same decoder for warming and drawing. All visible images
already bake synchronously before a worker's Ready packet reaches the presenter;
that existing transaction is the per-open raster readiness fence.

After the bar first presents, a lower-priority, coalescing worker queries Home
and the first apps page through the published engine contract. It retains up to
64 unique icon rasters, preserves engine order, refreshes on menu/scale changes,
and resolves recent IDs from `menu.preferences` against a transient bounded apps
reply. It keeps no query result or view alive between requests. A tiny offscreen Luau
view reads the same image dimensions as the visible launcher. The initial query
starts on the actual Panel Reload path before markup loading, overlapping that
work without bypassing restored routes or reply generation/revision checks.
Canvas initial-model priming alone was ineffective for this launcher; the
regression now proves that Panel Reload schedules exactly one correctly routed
query even when subsequent markup loading fails. Icon bytes are read afresh before
the content-keyed validation/raster lookup, including catalog changes at the
same path; a warm panel cannot retain a stale path-only byte snapshot.
The first menu projection waits up
to 20 ms for data; later typing never waits. Completion notification wakes
that first projection directly, replacing the former 1 ms sleep/poll quantum. Known invalid
or unreadable icons use the existing fallback, and a query timeout shows themed
skeletons. The worker is joined on shutdown and checks stop between work units.
Both grid and search-row variants are warmed for every selected icon. At scale
1.5 their pixels alone need about 1.21 MiB for 64 distinct icons, explaining the
cache allowance above the preferred 1 MiB rather than leaving recents cold when
the first keystroke switches to the row view.

The raster-only candidate still missed the expired-open timing gate. A private
release profile found 3.61 ms recompiling the fixed embedded Luau kit/installer
for each new frontend. The initial Panel-primed five-pair result still missed
the expired-open gate at +2.095 ms median. The fixed 7.5 KiB host adapter was
also being compiled from text on every view build; it now joins the kit and
installer in that same weakly shared immutable byte-buffer prelude. Frontends
share those immutable byte buffers with one resource charge; the live bar supplies their lifetime. Every frontend
keeps its own source compilation, Lua VM and store values, and the combined
8 MiB frontend limit remains. No compiled source bundle or view state is retained
by a new strong cache. The warmer returns freed temporary allocator pages before
sleeping, dropping its temporary catalog/markup objects; the measured cost also includes
thread, compiler/library and allocator residency.

`spikes/launcher-icons-proof.py` uses
64 private SVG/PNG desktop entries, 12 favorites and 8 recents, with the real menu
engine. It correlates every `PANEL_PRESENTED` with its accepted scene update and
model/raster readiness. This avoids the existing draw trace's first-two-seconds
suppression. Nested presentation is not physical laptop scanout timing.
Readiness counts are conservative scene/raster checks, rather than pixel-level
classification: an incomplete committed scene counts as missing even when the
growing reveal temporarily clips its empty slots. A ready scene has every image
raster validated before it can enter the presenter. The screenshots verify the
fixture SVG/PNG appearance; complex installed theme icons are not a timing sample.
No temporary product replacement or duplicate implementation is introduced. Both
arms use the same `clocks`, verbose image counters, frame trace and `--trace-state`
diagnostics. The reported process memory is instrumented; production per-frame
logging remains disabled by default.

Full CI passes on measured code commit `40b4055a7faf933fd87e62c712a38511090a6102`
in a remote Arch Linux container. The full `.github/workflows/ci.yml` equivalent
uses nice 10, Cargo two jobs and `RUST_TEST_THREADS=2`: formatting, warnings-denied
Clippy, workspace/trace/feature tests, Luau schema/checksum/analyser, Python
suites (including the new presentation scorer), packaging, wallpaper smoke,
private service lifecycles and independent license graphs all pass. Arch needs
the split `swaybg` package; the container's Sway capability was removed because
the capped container cannot execute that file capability. No host install changed.
The initial-priming regressions cover reply reuse, repeated priming, restored
route supersession and the actual Panel Reload entry point.

Paired QML/Bead screenshots use identical private 2880×1800 / 1.5 CPU sessions,
generated wallpaper/palette and launcher preferences. QML reference is
`090f92d8c9a275121a95bd3baa478b7a3251386b`; Bead screenshots use the initial
`40b4055a7faf933fd87e62c712a38511090a6102` measured candidate. The eight Home-state
images were refreshed after the actual Panel-path correction, with Bead then
refreshed again after main’s concurrent favorite-badge styling change and the
fixed-host bytecode change.

| State | QML | Bead |
| --- | --- | --- |
| Idle | [image](evidence/0129/qml-idle.png) | [image](evidence/0129/bead-idle.png) |
| Open | [image](evidence/0129/qml-launcher.png) | [image](evidence/0129/bead-launcher.png) |
| Hover | [image](evidence/0129/qml-hover.png) | [image](evidence/0129/bead-hover.png) |
| Pressed | [image](evidence/0129/qml-pressed.png) | [image](evidence/0129/bead-pressed.png) |
| Search (`a`) | [image](evidence/0129/qml-search.png) | [image](evidence/0129/bead-search.png) |

The additional search pair uses the same QML reference and Bead integration
`c487227fce00ee3336bc781a1ef412e5d1db7f1a`, with identical generated apps,
settings and query `a`. All ten screenshots were inspected. PNG/SVG row icon
sizes, order and spacing match visually.

Source declarations match a bottom reveal over 300 ms with OutCubic easing:
QML `NacreLauncherPanel.qml` uses `Easing.OutCubic`; Bead's `ease` panel
transition maps to `Curve::EaseOut`, `1-(1-t)^3`. The measured native opening
latencies below do not establish frame-by-frame animation or physical pacing parity.
Hover/pressed evidence targets the same generated first app tile at logical
(655,714), with 180/50 ms settling, rather than the different right-column items.

Known parity gaps: Bead's app icons are smaller and the first tile starts
selected. Main’s integrated hover-only favorite badges are preserved. The QML Recent block is absent from
Bead, and QML’s hovered-app detail column remains quick controls in Bead; quick-control availability text and update counts differ. Search focus and
selection/pressed styling differ; Bead’s selected app tile has identical hover/pressed pixels.
Search also lacks QML’s back/filter controls and keyboard-hint footer, uses a
different search-field outline/radius, and keeps quick controls in the detail column.
Both use the existing bottom attachment in these
captures. Palette, background and outer panel geometry match visually; pixel or
animation parity is not claimed. This unit keeps existing logical icon sizes.


## Idle-only cost and typing scope

A separate single paired CPU diagnostic never opens the launcher and samples
startup after 0.5 seconds and again after 123 seconds. It is informational,
separate from the five-pair phase medians used for the feature budget.

| Phase | UI unreclaimable control → candidate, MiB | UI delta, MiB | Daemon/provider tree control → candidate, MiB | Tree delta, MiB |
| --- | ---: | ---: | ---: | ---: |
| Startup | 26.627 → 35.100 | +8.473 | 41.246 → 42.082 | +0.836 |
| Never-opened, 123 s | 26.424 → 28.752 | +2.328 | 26.293 → 27.012 | +0.719 |

The early UI residency spike falls without opening a panel. The measurements
do not establish which allocator operation caused it. The daemon tree contains
three processes at startup and two after settling; external fixed fixture
providers are excluded. These are separate from UI residency. UI file PSS deltas
are −0.216/−0.225 MiB, and tree file PSS deltas are −0.397/+0.097 MiB.
[Numeric idle samples](evidence/0129/idle-cost.json) preserve measured components.

The first-key gate measures the first presented text echo after input; it does
not include waiting for the engine's final search reply. The separate final
results update is also reported below. The broader 8 ms results-ready CPU target
remains unmet in both builds. No whole-query 8 ms acceptance is claimed.


## Integration check

The rebased code at `c487227fce00ee3336bc781a1ef412e5d1db7f1a` includes
main's slider, wallpaper-readiness and panel-hover changes. Full current-workflow
remote Arch CI passes there: 655 workspace tests pass, one existing test remains
ignored, and all feature proofs, packaging, private lifecycle, wallpaper smoke
and independent license graphs pass. The one-arm [integrated native smoke](evidence/0129/integrated.json)
uses that exact release and shell source. Cold/warm/expired presentations are
20/21/20, all ready, with 55.535/14.635/61.743 ms first presentation and 3.189 ms
first-echo UI CPU. Generation loads are 1/0/1, confirming actual expiry/recreation.
Containment, zero OOM and zero host swap-in checks pass. This smoke is separate
from the frozen five-pair comparison and does not replace its matched control.

## Five-pair CPU result

Control is `ef7496f7a24ba2e561d63e4e64a7463680ffc9ec`; measured candidate is
`40b4055a7faf933fd87e62c712a38511090a6102`, after main integration, host sharing and completion notification. Both use
identical release flags and the same numeric bake diagnostic. The order alternates
A/C, C/A, A/C, C/A, A/C. Output is 2880×1800 at scale 1.5, CPU renderer.
The cold arm opens after the harness’s 0.5-second startup settle without a prior
panel open. The expired arm waits the actual 120-second production lifetime plus 3 seconds.
Each number below is a five-sample median; brackets are the full min–max spread.

| Open | Control first presented, ms | Candidate all icons/first presented, ms | Paired first-presented delta, ms | Control all icons, ms | Missing presentations control → candidate |
| --- | ---: | ---: | ---: | ---: | --- |
| cold | 62.204 [62.106–68.758] | 61.758 [60.634–63.708] | -1.040 [-6.703–1.553] | 110.228 [94.153–110.841] | 9 → 0 |
| warm | 14.642 [14.516–14.798] | 14.859 [14.457–15.200] | 0.081 [-0.229–0.684] | 14.642 [14.516–14.798] | 0 → 0 |
| expired | 59.714 [58.627–62.291] | 60.298 [59.196–60.947] | 0.713 [-2.499–1.410] | 107.061 [106.680–107.799] | 10 → 0 |

| UI phase | Paired unreclaimable delta, MiB | Paired anon delta median, MiB | Paired SHM delta median, MiB | Paired file PSS delta median, MiB (information) |
| --- | ---: | ---: | ---: | ---: |
| idle | 1.977 [0.457–7.336] | +1.977 | +0.000 | +0.460 |
| cold_open | 2.496 [1.145–6.707] | +2.496 | +0.000 | +0.207 |
| cold_closed | 2.570 [1.965–2.734] | +2.570 | +0.000 | +0.271 |
| warm_open | 2.547 [1.957–2.715] | +2.547 | +0.000 | +0.271 |
| warm_closed | 2.895 [1.477–3.324] | +2.895 | +0.000 | +0.271 |
| expired_closed | 2.074 [1.219–3.465] | +2.074 | +0.000 | +0.232 |
| expired_open | 1.961 [1.441–3.332] | +1.961 | +0.000 | -0.807 |

Unreclaimable is Pss_Anon + Pss_Shmem + DRM + SwapPss. These numbers cover the
UI process; daemon/provider totals are separate. Clean file-backed PSS is reported
only as information. No laptop absolute-cap acceptance is inferred from this host.
First-key echo UI CPU is 3.687 [3.584–3.928] ms (limit 8 ms).
The 64 selected grid/row variants need 1,264,896 pixel bytes at this scale; the
validation tiles and other shared small assets also use the 1,572,864-byte cap.
Per-arm warm-cache raster/charged counters are retained in the numeric evidence.
Additional measured residency includes the
idle service thread, shared compiler bytes and allocator overhead. This spends
more than the preferred 1 MiB to remove measured icon pop-in; compare the table
with this unit’s explicitly allowed 4 MiB budget.

The earlier raster-only five-pair candidate removed missing icons but failed
expired opening by +3.937 ms median. [Its numeric results](evidence/0129/raster-only.json)
are retained as rejected evidence. Weak prelude sharing and actual Panel query
priming reduced the timing gap, but the [Panel-primed five-pair candidate](evidence/0129/primed.json)
still missed expiry by +2.095 ms. Fixed host-bytecode sharing is included in the
frozen matched cohort, before the later main integration described above. [Final samples and spreads](evidence/0129/verified.json)
preserve all five pairs. Native presentation feedback reports refresh 0 (unknown);
no physical scanout frequency or laptop/live acceptance is claimed.


| First key, five pairs | Control UI CPU, ms | Candidate UI CPU, ms | Control input → presented, ms | Candidate input → presented, ms |
| --- | ---: | ---: | ---: | ---: |
| First echo | 3.208 [3.073–3.560] | 3.687 [3.584–3.928] | 3.589 [3.428–3.942] | 4.083 [3.959–4.304] |
| Final 64-row reply | 11.573 [11.450–12.298] | 10.917 [10.745–11.287] | 23.645 [22.556–24.924] | 22.449 [22.064–23.451] |

[Separate echo/result samples](evidence/0129/key-results.json) correlate the
input with distinct presented updates in all ten arms. The first-echo gate passes
in CPU and wall time; final results remain above the broader 8 ms CPU target in
both builds. Daemon CPU is excluded from these UI-process CPU numbers.

All 293 candidate presentations are raster-ready (98 cold, 95 warm, 100 expired).
Control has 9 cold and 10 expired incomplete presentations, with none warm.
[Ten capture receipts](evidence/0129/guards.json) confirm containment, zero OOM,
zero host swap-in, swap used zero before each arm, and generation-load counts
1/0/1 for cold/warm/expired. Every candidate open follows successful warming of
64 grid and 64 row rasters, with no fallback or query error: 1,283,140 raster bytes,
1,353,376 charged bytes against the 1,572,864-byte limit.

Startup remains variable: two of five paired idle samples exceed 4 MiB
(+6.887/+7.336 MiB), as do their cold-open samples (+6.707/+5.844 MiB).
All samples remain in the medians and spreads. Later close/expiry phase samples
are below 4 MiB. The separate never-opened diagnostic above records +8.473 MiB
early and +2.328 MiB after 123 seconds, without identifying a specific allocator
cause. This accepts the measured phase medians and smoothness trade; it does
not establish an instantaneous 4 MiB peak cap.

The [numeric provenance](evidence/0129/verified.json) pins frozen source trees,
binary checksums, the identical helper and numeric-diagnostic patch, proof hash,
flags and arm order. The measured candidate hash precedes rebase; the separate
integration evidence identifies the later build. Only generated fixtures,
screenshots and numeric summaries are committed; raw state logs stay private.

The final rebase preserves main’s dashboard release handling; the full CI workflow
is rerun on the final integrated tree before push. The launcher evidence stays
pinned to the exact measured and integration sources above.
