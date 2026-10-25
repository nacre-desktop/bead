# B0127 — Retained notification list and power dropdown

Status: available-window notification fix and compact power dropdown verified; full-history access is blocked on a Nacre contract addition.

The bell counts retained notifications, not unread notifications. QML uses
`NacreNotifs.retained.length`; Bead uses the daemon's `history_count` when known.
While storage is loading or unavailable and that field is omitted, the badge
now counts the available history window plus retained active rows, matching QML's
available cache. The separate `history_known` flag stays false.
The two-item menu was a consumer bug: `ControlCenter::notices` reversed the
already newest-first history and took only two entries, even for the full dropdown.

## Changes

The adapter projects retained active entries and the complete published history
window, preserving service order. Only the control-centre home preview limits
itself to two rows. The full notification dropdown scrolls all published rows. Its list is capped at
ten shared row units (440 logical pixels at default tokens), matching QML,
with DND/Clear fixed above the scrolling cards.
When the stored total exceeds the available rows, or `history_more` is true,
it explicitly reports how many of the stored records are shown. The badge keeps
the published total, so it does not silently redefine QML's retained count.

Power uses a compact, top-edge, widget-anchored dropdown on existing kit components:
Lock screen, Log out, Suspend, Restart, Power off. The existing `session` view is
shared by the widget and Alt+X / `nacre session`. The widget registry keeps
instance-owned dropdown routing; no changes to that concurrent work are needed.
The existing guarded session controller, inline confirmation card, timeout,
Escape cancellation and keyboard focus remain. No session action is executed
on a real machine by the validation.

Presentation and action order remain in Luau; Rust changes are confined to the
Nacre notification adapter and existing session dropdown factory. No new engine
primitive or temporary duplicate is introduced. Existing lazy creation and
closed-worker retirement are unchanged.
No new always-on allocation is intended; open notification data scales with the
existing published window. No matched five-pair memory measurement was taken; no memory verdict is claimed.

## Exact Nacre contract request (blocking complete count/list parity)

N0017 explicitly publishes a bounded history window and **no full-history paging
in v1**. `history_more` is a boolean, not a cursor or request. The published
`notifications-{lifecycle,storage,controls}.jsonl` transcripts use `snap`/`patch`
state with `active`, `history`, `history_count`, `history_more` and
`history_storage`; no history-page method is advertised. A watch can only show
that published window. The UI cannot reconstruct omitted records from it.

Please publish an additive, bounded history-page method with an opaque cursor,
ordered records, completion indication, and boot/source-generation invalidation
semantics, plus schema/transcripts and green CI. Define the count it enumerates:
current `history_count` also includes nontransient active feedback-filtered notices,
while QML's retained list and the producer's `retained` flag exclude them. Supply
an exact retained total (or clarify/change the count contract) so every badge
record can be enumerated. Bead will adopt the published contract; it will not read
the daemon's private history file or invent an unpublished call.

## Validation and evidence

The full `.github/workflows/ci.yml` check job was replicated by `.lead/ci.sh`
in the task-owned Arch container on prod: fmt, clippy with warnings denied,
workspace tests (630 shell tests passed after rebase, one existing ignored), feature/build
checks, Luau analysis/generated checks, all spike regressions, packaging,
wallpaper/services/private SNI proofs and cargo-deny for all framework graphs.
`RUST_TEST_THREADS=2` avoids an unrelated 50 ms Luau test-budget failure under
parallel load; no assertions, budgets or skips were changed. Arch needed
`swaybg` for the wallpaper smoke test, which then passed all checks.
Ten goldens were regenerated and visually reviewed, including 18-row scrolling
at scales 1 and 1.5 and all session/confirmation states.

Private nested CPU captures on the desktop used 2880×1800 at scale 1.5,
identical wallpaper, palette and settings, QML revision
`836df1a643eae6e2f54ec86967a83d2f27561d3d`. The
[verification record](evidence/0127/verification.json) pins Bead's captured source trees.
The primary notification witness reports badge 18, published rows 18, UI rows 18,
newest-first order and physical wheel scrolling to offset 1024. The toolbar
stays fixed and the oldest card is reachable. The power witness verifies the
actual power-widget anchor, Logout hover/focus, an unconfirmed inline prompt,
and Escape cancellation/closure. Reopening works; no session action was confirmed.
The separate hover retake's duplicate count watch had no snapshot and its
callback failed; it is not count evidence. The primary count witness is independent.
The capture helper currently fails to propagate callback-thread exceptions to
its exit code, so semantic acceptance uses explicit positive JSON witnesses.
Both capture resource guards report containment, zero OOM events and zero
host swap-in, with swap used zero before each arm.

The parity fixture now creates its notification state directories with mode
0700. Its former 0755 parent made native storage unavailable and omitted the
count; a private D-Bus reproduction verified ready/writable storage and count 3
after the fix. No real notification state was changed.

Paired evidence (QML left, Bead right):
[idle](evidence/0127/idle-qml-left-bead-right.jpg),
[notifications initial](evidence/0127/notifications-initial-qml-left-bead-right.jpg),
[notifications scrolled](evidence/0127/notifications-scrolled-qml-left-bead-right.jpg),
[power open](evidence/0127/power-open-qml-left-bead-right.jpg),
[pressed](evidence/0127/power-pressed-qml-left-bead-right.jpg),
[hover](evidence/0127/power-hover-qml-left-bead-right.jpg),
[unconfirmed prompt](evidence/0127/power-confirm-qml-left-bead-right.jpg).
Opening frame sequences were inspected ([Bead](evidence/0127/session-open-bead.jpg),
[QML](evidence/0127/session-open-qml.jpg)): Bead uses top reveal, 0.23 s ease-in-out;
QML retains its right-side dock animation. These are nested captures, not physical
scanout timing or a quantified smoothness benchmark.

Inherited B0111 visual gaps: notification cards omit QML app icons, time,
expand/dismiss controls and the Full panel toolbar action. These are not parity passes.
Known visual difference requested by this unit: QML's session route opens its
full control dock, while Bead's route shares the compact five-action power dropdown.

Other known differences: card typography, border/radius and density still differ;
the unrelated updates badge is clipped in Bead. The compatibility session route
opens the shared compact session view centered when there is no widget anchor;
the power button opens it anchored. QML's dock has four actions; this requested
Bead dropdown adds Suspend and its existing inline confirmation.
