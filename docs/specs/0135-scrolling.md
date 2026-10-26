# B0135 — shared scrolling

Status: implementation candidate; smoothness and open-memory acceptance fail.
Five-pair validation is complete. No live installation.
The updater bounds row construction to its viewport; notification cards and
Settings previously laid out ordinary repeated trees. All surfaces already route
through one Wayland handler, but it ignored axis source/stop, amplified finger
pixels threefold, inferred release after 65 ms, and carried old wheel travel
through direction reversal. The worker queued each scroll sample separately.
Native app windows additionally repainted and damaged their entire surface for
every offset update, independently of the panel renderer's viewport damage.
The dashboard notification and clipboard histories were clipped fixed columns
with no scroll target at all; both now use the shared viewport list. Their
adapters also discarded supplied rows at 12 and 7 entries. Those viewport-era
caps are removed; the existing published snapshot bounds still apply.

## Scope and acceptance

Use one source-aware policy: direct finger pixels, fling on finger axis_stop,
125 ms retargeted wheel ease-out, value120 precedence and immediate cancellation
of old motion on new input. Preserve action gestures (volume/workspace/tray).
No slider changes. General primitives remain independent of shell routes.
Offset-only scrolling must reuse layout; long lists must bound row construction.
Target reversal ≤ one frame, p95 frame cost ≤8 ms at synthetic 120 Hz and zero
missed cadence slots. Nested scheduling is not physical scanout certification.
Memory budget: no eager allocation or new always-on cache; reuse byte-accounted
materialized and renderer caches. Demand-driven capacity increases below are an
explicit smoothness trade. Report measured cost, never substitute zeros.
Newly reachable notification/clipboard rows have at most a 1 MiB open-view
unreclaimable budget; existing idle, close and scroll behavior keeps the
five-pair +0.5 MiB median regression gate. Any smoothness trade is explicit.

## Container inventory

| Surface | Implementation |
| --- | --- |
| Notification popup | shared Scroll; variable-height VirtualList replaces full Repeat |
| Dashboard notification history | shared VirtualList replaces a clipped, non-scrollable Column |
| Settings content / side tabs / selects | shared Scroll / variable-height VirtualList / overflow-only SelectRow Scroll |
| Launcher results / apps | kit.VirtualGrid |
| Updates / keybindings / picker | shell VirtualList |
| Dashboard clipboard history | shared VirtualList replaces a clipped, non-scrollable Column |
| Widgets editor | shared Scroll category, bounded gallery and detail forms (integrated B0114) |
| Welcome | shared Scroll canvas |
| Colors | VirtualList rail; shared Scroll canvas |
| AI chat | kit.Transcript window; shared Scroll questions; VirtualList saved chats |
| Control history / network / tray | VirtualList history / shared Scroll / variable-height VirtualList |
| Overview / wallpaper | outer and per-workspace Scroll / windowed tile gallery; carousel and spotlight are discrete selectors |
| Dashboard weather and artwork crops | shared Scroll (crops are bounded artwork) |
| Prompt / kit demo / layout example | VirtualList / shared Scroll demos |

The common input policy lives in bead-wayland/scroll.rs, not in per-view
animations. The variable-height option extends the existing shell VirtualList;
no second scrolling engine or retained legacy replacement is added. Same-direction
content samples coalesce in the worker; reversals remain ordered because summing
opposite travel changes clamping at the top/bottom edge. Action gestures retain
every physical input. The materialized cache updates descendant poses and nested
viewport clips, retaining offscreen scene descriptions inside the existing byte
budget. Virtual windows relayout only when their row set changes. Native windows and overlays
share semantic damage plus independent repaint history per SHM buffer. Inherited
`clip_children` and nested scroll clips survive cached pose updates. The existing
developer JSON fixture is still exercised by `spikes/measure.py`; it is not an
unused replacement or a second shipping scroll policy.
The framework/shell production delta is 1,205 added and 196 removed lines,
against main `d58dfc67`, excluding test-only files/`cfg(test)` items, proof
tooling, binary goldens and documentation; it includes eight deleted SVG lines.
Old per-view full-row
construction and the native full-surface repair path are replaced in this unit.
Wallpaper carousel/spotlight wheel events select the previous/next item; their
adapter excludes those strips from content-scroll regions. Volume, workspace and
tray actions likewise preserve one action per physical event.

The dispatcher applies an axis source throughout its own pointer frame, including
when a motion/button splits SCTK's adjacent axis grouping; it never carries the
source into a later frame. This follows the [Wayland axis-source contract](https://wayland.freedesktop.org/docs/html/apa.html#protocol-spec-wl_pointer-event-axis_source).

The 8 ms input diagnostic found a repeated 19 ms Settings frame: the first use
of the third SHM slot repaired the full window; another stale slot expanded a
viewport repair to 10 ms. Both use the existing pixel-copy helper now. A shared
buffer-history method adopts unchanged pixels from the last committed slot and
repaints its pending rectangle, keeping semantic surface damage unchanged.
It copies only when avoided repairs exceed one fifth of the frame, so small
caret/thumb edits keep their cheap path. It adds one optional slot index and no
SHM buffer or full-frame heap cache. Targets are conservatively treated as
nonzero, including after scale invalidation or a failed draw. Final timing and
memory validation of this repair follows below. Three final repeats confirm
viewport-only repair even on the third slot, but render p95 remains
10.226/9.997/9.620 ms and reversal takes 23.95/15.92/23.87 ms. Both targets fail.
Input intervals have p95 8.00–8.11 ms. Zero discarded feedback is not proof of
zero missed presentation slots; long boundary gaps remain in the results.
An 80-wallpaper hex-grid capture initially exceeded the retained frame's 64 MiB
raster limit before input in both control and candidate. Those arms are rejected
for performance acceptance. The two per-card SVG outlines are replaced with the
existing Polyline primitive and deleted; their geometry, strokes and palette
roles remain theme-controlled. Four hexagon goldens were regenerated remotely
and compared visually with the old rendering. Edge antialiasing changes slightly.
Retained image/text descriptions outside a persistent scroll clip now omit
their raster attachment. A newly visible pose prepares its raster before
publication; cached entries remain available for reversal under the existing
LRU limit. Animated reveal clips do not control resource lifetime. Pixel tests
at scales 1 and 1.5 cover enter/reverse and canonical open-frame reuse during
reveal; an 80-row synthetic scene would exceed 64 MiB if all rows were baked.

After those allocation reductions, scrolling crossed the admission limit at
67,450,436 bytes (64.326 MiB), only 341,572 bytes above 64 MiB. Images account
for 67,199,352 bytes and text for 251,084; two body images alone occupy
20,751,624 and 19,494,000 bytes. The bounded frame and GL image limits now allow
68 MiB, with 136 MiB for old/new CPU raster overlap. Allocation remains on
demand; the process-wide admission ceiling is 256 MiB and owner physical-memory
gates are unchanged. This uses
the standing smoothness-first memory-spend authorization. The native 80-image CPU
proof completes without UI errors, with offset 0 → 518 → 0, but render p95
is 54.763 ms and input-to-presented reversal is 329.53 ms: both fail. Its
single-arm unreclaimable memory is 38.715 MiB idle, 178.686 MiB open and
116.174 MiB after close+12 s; this is not a five-pair memory-gate result.
[Capture and exact source pins](evidence/0135/wallpaper-picker/candidate80.md).
The settled allowance grows by 4 MiB per CPU/GL set; CPU admission grows by
8 MiB to cover old/new frames. GL texture admission separately allows 136 MiB
while replacing a size-changed texture, whose old charge remains live until
upload succeeds. Its settled LRU still caps pixels at 68 MiB. Compared with the
old 64 MiB GL admission, this is 72 MiB more transient capacity, not an allocation
or a measured resident increase; global admission grows from 192 to 256 MiB. Actual cost
depends on demand and is measured separately.

## Evidence

The 120-card variable-height notification goldens cover top, bottom and immediate
reverse, with bounded row construction. Top and reversed pixels match; all three
were regenerated remotely and visually inspected. Cached/full layout tests cover
nested scrolling and clipping; input tests cover value120 precedence, finger 1:1,
wheel cancellation, exhausted ancestors and reversal after content shrink.
The dashboard regression starts with real protocol snapshots (120 notifications
and the published clipboard maximum of 50), checks projection readiness and the
full supplied row count, then reaches both ends with at most 12 materialized
rows. It also checks fixed headers, stable hit targets and shrink clamping.

Private captures use 2880×1800 at scale 1.5 with the same wallpaper and palette in
QML and Bead. A joined proof follows opposite injected input → delivered delta →
changed scroll store → matching epoch/update/generation commit → presentation.
Normal timing does not capture screenshots during input; visual sequences run
separately. Native app logs must be retained before another spawn truncates them.
The notification-history fixture closes its exact synthetic popup IDs before
opening history so an overlay cannot intercept its input. Bounded Settings finger
oscillation changes direction every three samples; wheel keeps realistic notch
spacing. Midpoint streams remain available for boundary/reversal stress.
Earlier streams performed a compositor roundtrip per sample and reached input
p95 intervals near 16 ms despite 8 ms deadlines. Their actual input intervals
remain reported. Dedicated cadence probes use nonblocking flushes, fail on
socket backpressure, and timestamp successful sends; a requested rate alone is
not proof that input or presentation reached that rate.

The private parent advertises 120 Hz, while installed nested niri 26.04
[`8ed0da4`](https://github.com/niri-wm/niri/blob/8ed0da4/src/backend/winit.rs)
advertises a 60 Hz Winit mode and reports `Refresh::Unknown` at submission.
Feedback timestamps use CLOCK_MONOTONIC; zero refresh means no prediction, not an
invalid timestamp. Measurements establish inner submission timing, not physical
scanout. The shared timer uses known feedback periods, or an 8.333 ms opportunity
when unknown; no benchmark-only rate override is shipped.

Full remote Arch CI passes on integrated `b6c61725`, including final global
admission and launcher input fixes. Five alternating CPU memory pairs are complete and fail the open-view gates;
the source-pinned per-surface timing evidence is linked below. Missing or
non-overflow measurements are not zeros. Whole-window gaps include time clamped
at a boundary; do not silently drop long gaps or call them active-motion misses.

## Fixture and parity limits

Only private synthetic data is seeded: notification IDs are closed explicitly,
clipboard contains text-only numbered rows, and extra nested Niri shortcuts run
`true`. No desktop clipboard, installed application inventory or live bindings
are copied into evidence. Content must actually overflow and the pointer must
hit its viewport before a timing result counts. A page that fits is N/A.

The published Updates producer supplies count/text/check time, not a populated
package list. Its long-package-list capture is unavailable; the existing bounded
UI fixture is not evidence of a live provider. No provider or ranking copy is
introduced for this unit. QML has no corresponding package-list viewport.

Inherited Settings differences remain: QML has Power & battery and Fonts launcher
rows, different dropdown/slider controls, and visible scrollbars; Bead uses its
existing controls without scrollbars. This unit changes scrolling policy and
bounded row construction, not those independent controls. Paired goldens alone
do not establish native QML parity or physical touchpad behaviour.
The paired popup images also preserve an inherited geometry difference: QML
uses the available output height, while Bead caps the notification stream at
720 logical pixels. Bead therefore shows fewer cards at once, with small card
padding/icon differences. Settings also lacks QML's trailing page label in the
app header. These are visible parity gaps, not scrolling acceptance passes.

The notification-history pairing also differs structurally: QML opens a right-edge
notification-card panel, while Bead opens its existing centered dashboard page
with a compact text history and status column. The supplied history and scroll
policy can be compared, but their row geometry and global hit coordinates are
not matched. The published screenshots retain this inherited parity gap.

Native capture also exposed a startup wiring bug: only core/extra dashboard
pages were registered, leaving published Clipboard/other optional routes absent.
Startup now adds only the missing Clipboard route; sources still open lazily.
The matched control receives only this same registration repair, preserving its
old clipped seven-row Clipboard body. Earlier unregistered Clipboard arms are
invalid open-view measurements and are excluded from the final cohort.

The default native-GL 80-image capture failed cache admission. Two later
attempts exposed a stale 64 MiB eviction threshold, then old/new texture overlap,
then process-global overlap. Rejected timings remain diagnostic in the
[picker evidence](evidence/0135/wallpaper-picker/default-gl80.md).

Clipboard also has an inherited structural gap: QML opens a bottom/right native
overlay with search, pin and delete controls; Bead uses the compact dashboard
history. The corrected QML y=900 visual probe shows movement, but its Qt path has
no joined state/presentation timing. Launcher QML uses Home/search/filter/detail
over a gradient; both measured Bead versions use Applications without that top
filter/detail pane over black, with different icons and spacing. No QML Widgets
counterpart was available. These are known parity gaps, not passing comparisons.

An initial registration attempt using every optional dashboard page exceeded
the existing 32-view catalog cap and was rejected before startup. That attempt
is superseded by the one-route repair; the catalog cap remains unchanged.
QML Clipboard requires its optional page to be enabled: an earlier `tab 6`
reference stayed on Dashboard and is not counted as Clipboard coverage.

A real 81-result native launcher probe exposed another existing input bug:
scrolling over result/favorite controls dispatched their activation action and
never reached the viewport. The runtime now clears those actions for axis input;
click/keyboard activation stays available. A regression reproduces the original
activation, then verifies real-runtime scrolling over both controls, reversal and
click activation after the fix. The virtual-grid layout hypothesis was rejected
by an actual 81-row hit-region test; no speculative layout change was applied.
Standalone panel apps also honor the existing trace-build `BEAD_TRACE_STATE`
flag, matching window apps, so AI scroll states can be joined in private proofs.
Both measurement arms receive that diagnostic-only flag propagation.

The final same-process GL80 finger → close+12s → wheel reopen capture passes
with zero UI errors at the 256 MiB global ceiling. Sampled cache totals are
149,161,023 bytes finger-open, 165,430,221 after close, 165,466,331 wheel-reopened
and 165,306,323 after final close. The limit change removes an observed reopen
failure; it does not allocate those bytes eagerly. Physical unreclaimable totals
(anon + shmem + DRM + SwapPss) are much larger: 38.47 MiB idle, 481.52 MiB at the
finger phase and 528.64 MiB at wheel reopen. The native GL process retains image
and composition residency; these are stress costs, not a laptop-cap pass or the
five-pair CPU reference. Render p95 is 0.862/0.739 ms, but input-to-visible
reversal is 281.93/559.72 ms and all-gap presented p95 184.27/1045.43 ms. Thus
functional reopen passes while smoothness targets fail.

The Niri-Winit path converts injected finger source to CONTINUOUS and drops
axis_stop. It proves pixel movement/reversal, not release kinetics. A separate
private direct-Labwc probe verifies wire FINGER plus axis_stop, post-stop motion
and opposite-input cancellation. Candidate has 48 distinct presented advances
after release, zero positive deliveries after opposite input and a first
negative presented offset; one stale positive state was never presented.
Input-to-visible reversal improves from 16.41 to 11.35 ms, still above 8.33 ms.
This is synthetic private transport evidence, not physical touchpad validation.
[Real release proof](evidence/0135/kinetic/direct-labwc/kinetic.md).

Results: [core Settings/notifications](evidence/0135/core-scroll/core-before-after.md),
[bounded active cadence](evidence/0135/active-cadence/active-cadence.md),
[rapid wheel](evidence/0135/rapid-wheel/rapid-wheel.md),
[Widgets](evidence/0135/widgets/paired-ba0.md),
[other surfaces](evidence/0135/surfaces/wide-wheel.md),
[Clipboard](evidence/0135/clipboard/clipboard.md), and
[CI/source provenance](evidence/0135/validation.md).

Final integration onto main `d58dfc67` preserves all 54 implementation/test patches
exactly. Main's idle native-buffer preparation now also checks the shared wheel
and fling animation state; it must not treat active scrolling as idle work. This
six-line integration guard is outside the frozen matched capture snapshots.

Attached AI transcript timing remains unavailable. The strict private pin probe
queries existing state after pin plus an outside click: QML reports
`leftPinned=true,left=true`, while both matched Bead control and candidate report
`target_open=false,chat.pinned=false,dismiss_active=false`. Wire button presses
arrive, but the pin interaction does not persist and the subsequent scroll
window is invalid. This is a concrete pin/hit limitation in both arms, not a
zero-cost or functional AI scrolling pass. No unrelated hover implementation is
ported into this scrolling unit.

Coverage is incomplete beyond the captured matrix. Nested selects, control
history/network/tray, Overview workspaces, weather/artwork crops, prompt and kit
demos have source inventory and automated coverage, but no native per-container
reversal/cadence result in this unit. Hover/pressed states and every optional
subpage were not exhaustively paired with QML. Missing evidence is not a pass;
the unit does not meet its every-surface smoothness acceptance.

## Five-pair CPU memory result

The source audit passes with zero errors for all five alternating pairs (7–11),
three fresh lifecycles per arm, scale 1.5 and close+12s. Both arms use CPU; the
gated sum is Pss_Anon + Pss_Shmem + DRM + SwapPss. File PSS is informational.

| Phase, candidate − control median | MiB | Verdict |
| --- | ---: | --- |
| Core idle | +0.2031 | passes 0.5 |
| Notification history open | +1.4844 | fails 0.5 and 1 MiB feature budget |
| Notification popup open | +1.2578 | fails 0.5 |
| Clipboard open | +2.1758 | fails 0.5 and 1 MiB feature budget |
| Eight-image picker open | −36.6055 | passes 0.5 |
| Largest closed+12s median (Clipboard) | +0.4258 | passes 0.5 |

The failed history/popup/Clipboard anonymous medians are +1.3516/+1.0820/
+2.1758 MiB; shmem adds +0.1328/+0.1758/+0.0000 MiB. Cache versus allocator
attribution is incomplete. Other listed shell phases pass. The new-history/Clipboard budgets fail; no
exception is assumed and no further performance optimization is accepted here.
Settings application memory is separate from the always-on shell: content/nav
open medians add 12.0312/7.3242 MiB. Their anonymous components add 6.8985/2.0781
MiB and shmem adds 5.1524/5.2461 MiB; causal allocation attribution remains
incomplete. These application costs are reported, not hidden by the shell gate.
[All samples, components, exact pins and limits](evidence/0135/memory/memory.md).

This cohort measures candidate203's non-AI pages. Later numeric AI-follow and
main idle-preparation fixes are outside its immutable source pin; no AI memory
result or full integrated-build memory acceptance is inferred.

The final AI probe exposed a separate concrete scrolling bug: follow-latest
snapped every offset within 48 pixels back to the bottom, preventing 7-pixel
finger samples from accumulating. The adapter now leaves following on any
reader offset change and resumes only at the actual bottom; thread changes
still follow the new thread. A regression fails before the fix, then covers
7-pixel and quarter-pixel movement, streaming while away, bottom resume and the
Latest action. All 13 chat tests and full integrated CI pass. The change adds no
state or cache; the immutable non-AI memory cohort remains tree203.

A separate published `pearls.open ai` window probe validates the final AI fix
on CPU: candidate315 moves 2249.27→1689→2249.27 while the matched control stays
stuck at 2249.27. The source/configured viewport and input hit are verified.
Render p95 is 2.093 ms; input-to-visible reversal is 49.22 ms (delivery 40.00 ms), so
functional scrolling passes and timing fails. Canonical all-gap presentation
p95 is 16.03 ms, with 89 missed-slot upper bound and unknown refresh prediction.
Endpoint images return to the start; movement is proved by joined state/commit/
presentation records. This is not attached-panel pin or QML parity acceptance.
[AI proof and fixture provenance limits](evidence/0135/surfaces/ai-pearl-transcript-203.md).
