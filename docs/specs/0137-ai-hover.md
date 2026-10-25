# B0137 — Keep hover-opened edge panels open

The outside-click backdrop covered the lip's input region. The pointer had not
moved, but Wayland sent a lip leave followed by backdrop enter at the same edge
coordinates. Recreating lips introduced another input handoff: the nested trace
showed replacement input arriving about 419 ms later, beyond the 130 ms grace.
Backdrops now subtract the shared lip regions from their input mask, and opening
a panel no longer destroys/recreates lips. Configure, enable/disable and settings
changes use one mask routine, preserving the bar exclusion as well.

`bead_core::edge_hover::EdgeHover` is the shared exit controller. It takes region
and focus flags, a clock and a caller-selected delay; it knows no views, settings,
compositor or layout. The native surface owner supplies control-panel contact;
the AI session supplies its existing lip/body lifecycle state. Either region
holds the panel; leaving both starts 130 ms grace, reentry cancels it, and a
focused text input holds it. Explicit opens, pinning and detached panels remain
exempt. Hover previews start without composer focus; explicit opens retain it.

`ShellControl::edge_input_regions` is the general backdrop pass-through primitive.
The shell derives it from the existing Luau frame config and settings. An AI child
reads the parent's lip declarations as input metadata without creating new lips.
The popup film uses vector cutouts from the same declarations so it cannot hide
stable lip artwork; overlapping rectangles are unioned before the even-odd fill.
The existing lip SVG paints its surface-color backing so cutouts reveal the lip,
not rectangular wallpaper holes. Disabled/missing lips are excluded from film
metadata as well as input masks; availability changes invalidate panel chrome.
No added dependencies, rendering surfaces/buffers or worker threads. Memory was
not measured. Failed view-owned-control and map-delay prototypes were removed.
No live desktop, installed shell or settings changes.

## Validation

Full Arch-container CI passed (`FULL_CI_OK`, exit 0), including fmt, clippy with
warnings denied, workspace/trace tests, feature builds, Luau checks, Python suites,
private service/tray checks, video smoke and root/independent framework cargo-deny.
Arch supplied Rust 1.99.1 rather than the pinned 1.99.0; cargo-deny was 0.20.2.
[Full output](evidence/0137/ci-output.txt), [commands](evidence/0137/check.sh) and
[validated source hashes](evidence/0137/source-sha256.txt). Focused tests cover lip/body/focus
holds, timed exit, reentry, explicit-open exemption, rounded layer input geometry,
child metadata without duplicate lips, and transparent film cutouts that preserve
the body. Chat/control goldens were regenerated and inspected; chat was unchanged,
and the two control overview updates only change a small slider fill.

The private proof uses the desktop lock, private HOME/buses and existing swap,
containment and watchdog gates. Bead runs 20 trials per panel, with one second on
the lip, at least five seconds inside the body, and closure after exit. The
composer is focused using real Tab input and checked outside for five seconds.
The QML reference (`37583d61928368bc22a46aa806fd0a2e06cfee55`, frozen
origin/main) uses the same generated wallpaper/palette and geometry. Exit times
are socket-poll observations at roughly 200 ms cadence, not presentation latency.
Nested refresh and screenshot sequences are not physical scanout measurements.


Final release proof passed all 40 trials; the focused composer stayed open outside
for at least 5.1 s. Both arm guards passed: zero OOM events, verified containment,
no swap use and no host swap-in. [Results and binary hashes](evidence/0137/summary.json),
[trial records](evidence/0137/bead-trials.json) and [paired states/opening samples](evidence/0137/gallery.md).

| Panel | Passed trials | Shortest body hold | Median logical-exit observation |
|---|---:|---:|---:|
| AI sidebar | 20/20 | 5.121 s | 0.201 s |
| Control | 20/20 | 5.130 s | 0.201 s |

## Visual comparison limits

Matched 2880×1800 CPU captures at scale 1.5 show matching panel placement,
rounding, joined contours, palette roles and chevron direction. Existing content
parity gaps remain: QML explains that AI is optional while Bead reports “Request
failed”; Bead disables Brain and some control actions in the unavailable fixture.
QML includes VPN and arranges power/stay-awake/mic-mute tiles differently, while
Bead omits VPN and spans mic-mute across both columns. QML shows notifications
loading where Bead shows an empty list; its theme choice is enabled/selected.
Some button/text widths and the fixture package count differ (150 versus 15).
These gaps are outside hover lifetime; screenshots preserve them for review.

Both sampled openings enter from their screen edge without a blank panel or
reopening observed. AI transition phase differs slightly: the 250 ms-after-close
Bead sample retains a narrow closing tail where QML has finished; subsequent
control states confirm it is gone. Pressed samples also catch different opening
phases. These coarse samples do not establish identical duration/easing or prove
refresh-rate pacing. The requested focused-input hold deliberately exceeds the
QML reference, which closes on pointer exit even with its composer focused.
