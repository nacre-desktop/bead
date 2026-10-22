# B0020 — Resident notifications and OSD

Status: **Implemented; review.** Consumer of Nacre N0017 (default-action marker
published at `83d73952`) and N0036 (brightness reply level, `40ba5d9b`). Gap-list
unit 5 / Roadmap Feature 9. No live ownership or switch authorization.
Runtime pin: nacre-protocol `652d6c4f` (contains the default-action marker; N0036
only adds JSON reply fields Bead reads untyped, so no repin). Keyboard-layout OSD
is deferred (no contract).

## Objective and boundaries

Make notifications, history, DND and OSD resident Nacre products, independent of
whether a panel is open. The daemon owns notification state, native bus ownership,
expiry and durable history; Bead consumes published snapshots and methods.
Views own expansion, scrolling and animation only. Never add a second history
writer, settings writer, notification provider or guessed protocol method.

Framework mechanisms remain generic and permissively licensed. Nacre appearance,
placement, Orient roles, daemon routing and policy belong in GPL `nacre-shell`
and `shell/`. ADR 0004 applies; any newly exposed framework assumptions go in
the ROADMAP Generalization list, without unrelated generalization work.

## Published producer and remaining contract gate

Nacre `f468060038edfc33b99a1b06bf773ea9ffd90b7e` publishes the managed
notifications producer, protocol types/schemas and six generated transcripts:
`notifications-{lifecycle,controls,errors,loss-restart,ownership,storage}.jsonl`.
N0017 owner decisions accept separate managed ownership opt-in, bounded admission
and history with no paging, frozen configured QML timing, dismiss-retain/explicit
forget-delete, and immediate Notify acknowledgement with asynchronous storage.
The earlier design-proposal/policy gate is cleared by the owner's implementation
request. Bead remains a socket consumer; no ownership fallback or live switch.

Default action: `Notification.default_action` (opaque action ID or null) is hidden from
the button row and invoked on card click only when action-on-click is on. The policy has
no published runtime source, so the frozen QML default (off) holds; `--notification-action-on-click`
is a launch flag, not a settings writer.

Keyboard-layout OSD is a follow-up, explicitly outside B0020. Existing published
volume/brightness topics are in scope; keyboard backlight is not keyboard layout.
Other QML-only OSD channels require their own published contracts before adoption.

## Behaviour to match

Read the QML from the Nacre revision above, using `git show origin/main:<path>`.
Paths below are relative to `nacre/shell/`.

| Behaviour | Reference and acceptance |
| --- | --- |
| Stack placement | `modules/drawers/NacrePanelHost.qml`, `modules/notifications/NacreNotificationStack.qml`: usable top-right, 424 px outer width capped by parent, 12 px insets, 8 px spacing, height bounded to parent minus 24; clipped scrolling. Match stacking order, replacement position and output selection against QML. |
| Transitions | Stack reveal 230 ms OutCubic; card insertion fade 160 ms. Retire exiting rows and hover state after hide/destruction; no permanently retained view resources. |
| Expiry/replacement | `services/NacreNotificationEntry.qml`: replacement keeps identity, updates content and restarts presentation; native positive timeout closes; -1 resolves to the configured QML default for every urgency. Zero has no native expiry but hides its popup after that configured default (currently 5000 ms); only positive timeouts >10 s use a separate 10 s popup clock. `expire:false` disables both clocks. Hover pauses both producer clocks; Bead never computes expiry. |
| Notice/actions | `modules/notifications/NacreNotice.qml`: plain text, critical/error border; expanded summary/body limits 6/12 lines; expand/dismiss, right-click and swipe >=30% width dismiss. Default action omitted from buttons; card invokes it only when action-on-click policy enables it, otherwise expands. Nondefault actions shown expanded; history has no live actions. |
| Suppression | DND plus UI inhibition hides popups; leaving suppression never replays. Producer expiry continues. Panel/session/OSD suppression follows PanelHost. Clear hover on hidden/destroyed surfaces and provider disconnect. Native inhibition is tested separately; it is not established by QML visuals alone. |
| History/DND | `modules/bar/popouts/NacreHistoryPopup.qml`, `modules/controlcenter/` notices, `services/NacreNotifs.qml`: newest first, loading/empty/error distinct, DND uses sole daemon settings writer, clear scope follows published contract. Dismiss retains; only explicit Forget (`forget:true`) or clear deletes. No mutation through old direct settings/history paths. Display the bounded newest history window, total count and `history_more` separately; loading, bus health and `history_storage` readiness/writability/errors remain distinct. No v1 pagination. |
| Persistence | Preserve Rust daemon's 30-day retention, existing version-1 formats and exclusions. Active retainable text restores as inert history, with no popup/action/deadline revival. Test transient/feedback exclusions, corrupt/readonly/future files, failed saves, clear-before-load tombstones and restart. Never replace this with Bead's current 256-row/3 MiB fixture store. |
| OSD | `modules/osd/NacreOsdEvents.qml`, `NacreOsd.qml`, `NacreLevelNotice.qml`: initial-state arming 400 ms, coalesced changes, focused output, bottom overlay, frame+24 px margin, width 200..336, no keyboard focus, 150 ms fade and 2000 ms hide. Hover pauses hide and leave restarts it. Volume/brightness use real published state; repeated equal-valued events use serial identity, expired/reconnected state never replays. Keyboard layout is deferred to a follow-up contract. |

## Implementation (as built)

- `nacre-shell/src/notice_service.rs`: daemon consumer (topics `notifications`, `osd`,
  `audio`). Pure projection of the last-good snapshot (stale/unavailable/loading/error
  distinct), revision-checked plans for dismiss/forget/action/DND/clear, local expansion,
  hover (`set_hover`) and OSD-visible suppression (`set_inhibited`, sent from a worker, never the
  transport callback), OSD arming/serial/expiry/2000 ms hide with hover hold. Backend trait
  `Notices` shared with the private compatibility owner. Launch: `nacre-shell --notification-daemon
  SOCKET [--notification-provider] [--notification-action-on-click]`; never owns a bus name.
- Views: `shell/notifications/main.luau` (card click, right-click dismiss, default hidden),
  `shell/osd/main.luau` (whole card hover target), dashboard Notifications page
  (DND, Clear history, Dismiss/Forget rows, bounded-window note).
- OSD sources: `osd` topic when the daemon publishes it (none yet); else default-output
  volume from the `audio` topic; brightness from the `brightness.adjust` reply
  `level_percent` (N0036), forwarded by the bar's key route to the notice process
  (`bead.osd show brightness`). Brightness without a value is never shown.
- Not done / limits: swipe-dismiss (no gesture event in the framework); popout history
  from the bar (history lives on the dashboard page); panel/session suppression (needs a
  cross-process signal; only OSD suppression is sent); the daemon allows ONE UI provider,
  so hover/suppression work only when the notice process is the provider
  (`--notification-provider`) and the bar/launcher process is not: a Nacre contract decision.
  Clear history uses scope `history` (closed records). Keyboard-layout OSD deferred.
- Generalization list additions: bead-ui host-action allowlist is hard-coded (`script.rs`)
  and should be host-registered; `PointerRegion` reaches every panel session.

## Evidence (2026-10-22/23)

- Prod Arch container: fmt, clippy `-D warnings`, 266 nacre-shell tests incl. 22 notice_service
  tests (published lifecycle/storage/loss/ownership transcripts replayed, fake-daemon calls).
- Nested smoke on the desktop (private dbus, labwc+niri, real `nacre-helper daemon
  --notifications=daemon` from Nacre `652d6c4f` on the private bus, `spikes/notice-daemon-smoke.py`):
  32/32 checks: popup, default hidden, hover pause/resume, expand, ActionInvoked, right-click
  dismiss retained, DND no replay, shell restart, daemon loss (name unowned, no fallback), real name untouched,
  OSD (baseline, serial repeat, 2 s hide, hover hold, brightness gate, expired, caps). OSD source was a fake
  `osd` producer (daemon has none).
- Memory (desktop, CPU tiny-skia, debug stripped, medians of 5, 2 runs each): candidate minus control
  Pss idle +1.9 MiB, one popup +2.7, closed +2.3 (anon +1.6..1.9); daemon separate ~56 MiB.
  This FAILS the +0.5 MiB relative gate; cause not yet isolated (link thread/runtime, protocol
  decoding). Control hosted the bus in-process. Owner/lead decision needed.

## Acceptance and stop conditions

- Deterministic lifecycle tests: admission, replacement while hovered, urgency,
  native close reasons, zero/positive/default expiry, popup-only hide, default
  and explicit actions, resident behavior, stale revision/action rejection,
  DND, independent inhibition and owner/provider disconnect cleanup.
- Published-schema fixtures cover absent capability, startup/unavailable/stale
  state, loss/resync, slow subscribers, disconnect/restart/new boot, no mutation
  replay and no popup replay. Preserve existing protocol-v1 history/popup/OSD
  fixtures and `bead-services/tests/notification_api.rs` reference coverage.
- Private `dbus-run-session` ownership tests: one owner, competing owner refusal
  with DoNotQueue/no replacement/no queue, exact-owner release, restart and
  consumer-only UI. Use private HOMEs/state. Never use a real session bus.
- Rehearse coexistence/handoff only as design here: fence old admission and
  writer, flush, release, acquire, verify consumer-only readiness; rollback
  never starts two writers. An unrelated owner means refusal. Live switch,
  activation files, units, install and deployment remain owner gates.
- Full `.github/workflows/ci.yml` equivalent in a uniquely named Arch container
  on PROD via SSH/tmux: fmt, warnings-denied Clippy, workspace/trace tests,
  boundary check, renderer configurations, facade/headless/Luau schema,
  capture regressions, private services/tray lifecycle and all cargo-deny graphs.
  Commands `nice -n 10`; Cargo -j2. Remove task containers/images/build directories.
- Nested smoke only on the desktop under `desktop-heavy.lock`, waiting for
  peers to release it. Match QML stack/history/OSD at real geometry and scale,
  exercise input, expiry, suppression, restart and lazy close on both compositors.
  No laptop tests/builds. Before captures apply swap/quiet, containment/watchdog
  and memory gates; report PSS+DRM+SwapPss with unknown DRM kept unknown.
  Desktop comparisons use the documented relative memory gate; no absolute
  laptop acceptance claim. Stop for unexplained anonymous growth >1 MiB.
- Stop for missing published contracts, unresolved policy, live-only validation,
  memory/safety failure or ownership ambiguity. No speculative framework work.

## Temporary duplicates and expiry

- `bead-services` private Server/Center history and shell private-owner route
  remain compatibility fixtures, not production fallbacks. Delete the replaced
  shell owner route **7 days after the daemon replacement is trusted**: published
  CI, private ownership/storage/restart tests and accepted nested parity. Keep
  the generic hostable service and explicit standalone tests; they are not duplicates.
- QML always-on notification/history/OSD views remain the daily-driver reference.
  Delete **7 days after the replacement is trusted**, defined as owner-approved
  core switch plus a stable week and rollback review. Removal needs owner approval.
- This contract-audit stage creates no new implementation duplicate.

## Execution receipt — 2026-10-22

Audited the exact producer pin, protocol action types, native-to-opaque mapping,
and frozen QML default-action/timer policy via read-only `git show origin/main`.
The general producer/policy blocker is resolved; default-action parity remains
contract-blocked. Incomplete consumer/assets drafts were not integrated. No
product lines, runtime pin, bus ownership, live state or installed files changed.
Full Bead CI receipt is recorded in STATUS; no nested or memory result is claimed
for this documentation-only update. Resume with the additive upstream marker,
then complete the implementation and original private/nested acceptance checks.
