# B0111 — Daily-use sidebar, tabs and notification bugs

Lead accepted integration under SMOOTHNESS FIRST on 2026-10-25.

> lead-accepted 2026-10-25: Today/Quick closed deltas +0.6..+2.6 MiB; consolidated memory review follows

The failed measurements and original open/closed phase labels remain recorded.
The notification fix is already on main; the remaining patches are rebased onto
B0113 widgets, B0118 daemon recovery, B0120 pill spacing and B0119 focus fixes
for integration.
No live shell, installed configuration or daemon contract changes are included.

## Notifications

The UI-provider holds `notifications` as a permanent subscription. Both the
notification popup and control-center factory tried to lease it again;
`lease_topic` rejects permanent topics, and `filter_map(...ok())` silently
removed the source. The view therefore had no history and stayed on Loading.
The adapter now shares a permanent source or acquires a scoped lease when absent.
Launcher Quick uses the same acquisition rule (its previous rule was already correct).

The daemon-transport regression includes existing stored history, readiness,
count, clear availability, close/reopen and keeping the permanent subscription
while releasing only the view's controls lease. It and the existing permanent
Launcher Quick subscription regression passed in the private CI container.

The private daemon-mode reproduction uses one synthetic history row, saved
before startup, with both `--notifications=daemon --behaviours=daemon`. In the
reported control build the service is running, the bar count is one, but the
panel stays on Loading. The candidate renders the saved summary and body,
without Loading, while keeping the service running with zero failures.
The launcher Quick projection has three controls and `launcher.quick_error=""`;
its screenshot shows no error. `launcher.quick.error` in a bake log identifies
the error text node, not evidence of a nonempty error. No extra Quick fix was
needed beyond sharing the existing source-acquisition helper.

The first QML fixture arm also stayed on Loading, for a different setup issue:
its daemon-helper mode sends the one-shot history read before the helper
transfer handshake is ready, and QML does not retry that failure. The fixture
path and fields match QML. Disabling the daemon-helper lane prevented this
capture's UI provider from starting. The narrow reference instead uses a private
fixture copy with only the reader's startup gated on `NacreHelpers.ready`, and
waits for `notifs counts` history readiness and a retained row. Its patch is
recorded and the reference is labelled as instrumented; the checked-out Nacre
source remains unchanged. The instrumented reference reports `historyReady=true`,
`retained=1`, empty error, and renders the fixture. The unmodified reference
failure is not a parity pass.

Existing visual differences remain: QML has a larger header, a Full panel action,
and app-icon/time/expand/dismiss details on history cards; Bead uses the existing
compact control-centre notification row. This unit fixes source delivery, not
that pre-existing presentation gap.

## Sidebar

The external app route lost the hover-open origin, and panel pointer-region
notifications went only to the bar runtime. The sidebar now receives those
notifications and uses Luau-defined transient/pin/detached rules with QML's
130 ms leave grace. Explicit click/key opens stay open; close/Escape clears pin.
The lip and body are separate regions: either cancels the pending close.
An ordered, small-stack adapter lane waits for the cold app's prepared scene
before delivering queued lip observations. This avoids dropping a leave during
startup or letting it overwrite a later body enter.

Regression checks cover explicit opens, no-visit lip departure, reentry, pin and
unpin, detached panels, close resetting pin, and both startup ordering races.
Static layout is unchanged. No daemon schema changes or extra provider are added.

Rebasing onto B0110 exposed another lifecycle issue: its 120-second app cache
keeps the child alive after close, while the parent previously removed external
panel geometry only when that process exited. The parent therefore kept treating
the closed sidebar as open and blocked lip reopens. Cached geometry and current
visibility are now separate. The child sends ordered visibility observations over
the existing private app IPC lane; late geometry cannot resurrect a closed panel.
This preserves warm workers rather than reverting the smoothness cache.
Regressions exercise close/reopen ordering, hidden cached geometry, late geometry,
and validation of the private visibility route.
Seven final nested states pass: hover departure, cancelled dwell, click persistence,
body hover/pressed, physical cached hover reopen, and pin/outside/unpin closure.
Raw app state confirms the cached process survives close.
[State proofs](evidence/0111/sidebar-verification.json) distinguish these passes
from failed Super+A input injection: child-seat and parent-seat attempts never
invoked the logged helper wrapper. Physical keybind delivery is unvalidated;
that failure does not establish a sidebar command-routing defect.

Capture cases close the external app directly between states: the existing
`nacre close` route closes resident bar panels only, so it is not used as proof
that the external sidebar closed. That existing global-close routing gap is
outside this hover/click/key/pin change.

The pinned reference and candidate have matching panel placement. Existing
capability presentation differs: Brain and composer attachment/screenshot actions
are enabled in QML but disabled in this Bead fixture. The warm pin test seeds
the conversation twice, so its extra message pair is fixture content, not a
layout parity claim. Paired pin crops therefore compare the common header.
Small existing header text-baseline and tab-width differences also remain visible.

## Dashboard caption alignment

The old pearl followed the centre of the icon-plus-caption group. This also
matches QML's current group-centred navigation, but does not meet this unit's
explicit request for a pearl under the text. The Luau layout and both selection
actions now exclude the icon and gap, moving the pearl 13 logical pixels right
when captions are present. Icon-only navigation keeps its previous centre.
This is an intentional difference from QML, not a claim that QML centres text.
A regression compares the actual caption and pearl scene rectangles for all
four pages at scales 1 and 1.5, including navigation between pages.

Real-font 4× crops at scale 1.5 put the candidate pearl relative to the
brightness-weighted visible ink centroid at Today +0.36, Media +0.40,
Weather +2.10 and System +1.01 physical pixels. QML is −17.72, −18.16,
−15.97 and −16.99 pixels respectively. Positive means pearl right of ink.
The caption advance box is the layout target; asymmetric glyph ink explains
why its brightness centroid need not be that box's centre.

The paired crops also retain existing differences: QML highlights the hovered
selected tab, whereas Bead's selected tab is not interactive and has no hover
pill; Media/System glyphs differ. Those gaps are recorded, not claimed fixed
by this placement-only change.

## Status peek motion

QML uses a 230 ms InOutCubic height reveal, without a fade. Bead already declares
the same motion. Private nested entry sequences from the reported control build
show a height reveal for audio, network, Bluetooth and battery; adding another
fade would change the reference motion rather than restore a missing effect.
At 25 ms screencopy sampling, image-progress 10–90% spans are QML
100–101 ms and Bead 76–102 ms. These are image-distance proxies, not contour
geometry or physical scanout timings. Native captures cross the 5% image-progress
threshold 232–261 ms later (median 246.5 ms); edge pixels can appear earlier, so
this is not exact first-pixel latency. These reported-control gaps were assigned
to B0110. The sampled native progress is also coarser:
audio repeats a partial-height pose before jumping to full height, whereas QML
has more intermediate poses. This is not a claim of smooth-motion parity.
Earlier exit sequences started before the
native pane was fully prepared and are excluded from duration claims. The final
integrated renderer refresh was stopped at the memory gate; these earlier
captures do not establish its motion parity.

## Validation

The full remote Arch CI-equivalent passes on `4f9590b` (production binary from
`6e05430`; the difference is test-only). It ends `CI_ALL_OK`, including 602
nacre-shell tests, renderer configurations, Luau analysis, packaging/lifecycle
checks and all dependency/license graphs. All 36 refreshed dashboard goldens were
visually reviewed. [Exact commands and results](evidence/0111/checks.txt).
Captures use private nested sessions at 2880×1800, scale 1.5, CPU rendering,
the same wallpaper/palette and real fonts. No laptop or live-shell acceptance
is claimed. Raw harness failures remain private and are not product evidence.
Early memory attempts used a superseded dashboard-only driver; later pilots
used empty history. Neither set counts toward N0068 acceptance. The primary
series requires both daemon takeover flags, one mode-0600 saved fixture row,
and a bar count of one before the initial sample, then all 13 prepared/settled
phases in five alternating pairs.

The complete pre-B0110 comparison uses the reported `4f8db4cf` control and
`eb64fe62` candidate, with the same default-debug stripped CPU build and helper.
All five alternating pairs pass all 13 phases: the largest paired median
unreclaimable increase is notifications open, +0.351562 MiB (+0.164 anonymous,
+0.188 shmem). Closed-sidebar increase is +0.062500 MiB. DRM and SwapPss are
zero; file-backed PSS is reported separately.
[All phases and paired deltas](evidence/0111/memory-before-b0110-table.md) and
[source/fixture proof](evidence/0111/memory-before-b0110-provenance.json) retain
the exact comparison. These numbers predate the renderer rebase and do not
claim to validate its 120-second warm cache.

The first warm-cache comparison (`2e523f2` control / `6e05430` candidate)
completed five pairs and 15 phases but **failed**: Today open is +0.539 MiB
resident shell / +0.645 MiB shell plus cached chat; Quick is +0.746 / +0.813 MiB.
The excess in these phases is anonymous memory; DRM/SwapPss deltas are zero.
[All paired snapshot results](evidence/0111/memory-warm-snapshot-table.md) and
[raw changing-sample examples](evidence/0111/memory-warm-snapshot.json) are retained.

Those windows start only 0.4 seconds after preparation. Today anonymous memory
changes by several MiB inside them, and closed Today changes shared-memory
buffers inside its window after a 2.5-second wait. They do not establish settled
memory. The predeclared follow-up adds an extra fixed three seconds before every
phase's
five samples, keeping both roles, sources, fixtures and the +0.5 MiB gate unchanged.
No adaptive waits or threshold changes are used. Three matched pairs completed:
Quick shell deltas are +1.508, +2.504 and +1.086 MiB; combined UI deltas are
+1.605, +2.594 and +1.191 MiB. Those Quick windows are flat. Even if the remaining
two pairs were arbitrarily negative, the planned five-pair median would remain above the gate
(lower bounds +1.0859375 MiB shell / +1.19140625 MiB combined). Remaining captures were stopped
under the repeated-growth rule; no five-pair follow-up aggregate is claimed.
Today control pair 3 still varies by 1,956 KiB inside its delayed window.
[All delayed pair/component data](evidence/0111/memory-warm-delayed.json) and
[summary](evidence/0111/memory-warm-delayed-table.md) retain these limits.
The extra Quick cost is shell anonymous memory; file PSS, DRM and SwapPss do not
explain it. Its cause is not isolated. Both roles already use B0110's 120-second
cache, so the measurements do not isolate a new cache-policy cost. The lead
accepted integration despite the deltas; the consolidated review will investigate
them. The earlier failed five-pair snapshot remains recorded.
The follow-up's third candidate arm hit a UI command deadline while closing Today;
its log records a 1,013 ms panel-frame processing interval and no OOM. It produced
no memory record. The whole matched pair was retried once with identical settings,
preserving the interrupted control/candidate attempt separately. The retry
completed both arms. This timeout remains a known runtime failure, independently
of the memory stop. No further keybind/peek captures followed the stop.

Product source changes total +481/−73 lines across the standalone notification
fix and this unit's remaining patches, including comments and blank lines,
excluding tests, docs and golden images. The engine addition is a reusable
behavior-controlled pointer-leave deadline; Nacre app IPC remains at the GPL edge.

Private capture harness/source copies are temporary: delete 7 days after the
replacement is trusted. Remote copies were removed after evidence transfer; owned jobs, scopes and
containers were cleaned up. No live session or installed configuration changed.

## Integration review — 2026-10-25

Rebased onto `a396a83`, retaining the B0113 widget port, B0120 spacing and
B0119 keyboard-focus fixes. The only rebase conflict was appended ADR notes;
both the text-width and pointer-region generalization entries are retained.
The sidebar runtime/edge routes and all three caption-pearl calculations carry
over unchanged; status widgets keep their stable peek kinds and shared reveal
configuration. Existing evidence remains pinned to its measured source revisions.
Fresh full remote CI is required for this integrated tree; no new desktop capture
or live-session change is part of the lead integration decision.
