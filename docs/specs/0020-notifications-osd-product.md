# B0020 — Resident notifications and OSD

Status: **Blocked before implementation — producer publication and policy decisions.**
Gap-list unit 5 / Roadmap Feature 9. No live ownership or switch authorization.
Spec first, 2026-10-22. Fetched Bead `origin/main` is `155709c`; its highest
spec is B0018. Reserve B0019 for the concurrent launcher worker and use B0020
(highest observed + 2), as requested. No push from this task.

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

## Contract gate — actual evidence

Nacre fetched `origin/main` `aa32dbf82b66570c74b3774101f954de7ce0367a`:
`docs/specs/0017-daemon-notifications-contract.md` explicitly says **design
proposal**, with no runtime/schema publication or consumer adoption authorized.
Its phase D requires usable producer publication plus green CI before Bead
pins/adopts. Standing rules likewise prohibit adoption of reserved capabilities.
This task does not authorize editing Nacre or implementing that proposal here.

Published reserved types in `crates/nacre-protocol/src/core_services.rs` include
NotificationsState, popup_order/inhibited, history storage and OsdState. They
do not establish a usable notifications producer or its proposed six methods.
Notification revision/retained/paused and history_more in N0017 are proposals,
not current published fields. OSD channels are volume, brightness, microphone,
capslock; there is no keyboard-layout channel. The existing QML `keyboard`
channel is keyboard **backlight**, not layout.

Unblock only after Nacre publishes producer source, schemas/transcripts and
exact-source green CI; record its revision and capabilities before coding.
Resolve default expiry/popup policy and dismissal/forget mapping through N0017.
Obtain a newly agreed and published keyboard-layout OSD contract/behaviour, or
an explicit owner decision deferring layout. Do not reinterpret it as backlight.
Full QML OSD parity also includes output selection, keyboard backlight, DND,
night-light and power profile; these are absent from the current OsdChannel
enum and require published contracts before adoption. Microphone/capslock are
already represented, but their presence alone does not prove producer support.

## Behaviour to match

Read the QML from the Nacre revision above, using `git show origin/main:<path>`.
Paths below are relative to `nacre/shell/`.

| Behaviour | Reference and acceptance |
| --- | --- |
| Stack placement | `modules/drawers/NacrePanelHost.qml`, `modules/notifications/NacreNotificationStack.qml`: usable top-right, 424 px outer width capped by parent, 12 px insets, 8 px spacing, height bounded to parent minus 24; clipped scrolling. Match stacking order, replacement position and output selection against QML. |
| Transitions | Stack reveal 230 ms OutCubic; card insertion fade 160 ms. Retire exiting rows and hover state after hide/destruction; no permanently retained view resources. |
| Expiry/replacement | `services/NacreNotificationEntry.qml`: replacement keeps identity, updates content and restarts presentation; native positive timeout closes, zero has no native expiry, -1 resolves to 5000 ms in QML. Popup-only hide after 10 s for no-expiry or >10 s notices. Hover pauses both clocks. Bead's current critical -1 persistent policy differs: require the producer policy decision, never silently change it. |
| Notice/actions | `modules/notifications/NacreNotice.qml`: plain text, critical/error border; expanded summary/body limits 6/12 lines; expand/dismiss, right-click and swipe >=30% width dismiss. Default action omitted from buttons; card invokes it only when action-on-click policy enables it, otherwise expands. Nondefault actions shown expanded; history has no live actions. |
| Suppression | DND plus UI inhibition hides popups; leaving suppression never replays. Producer expiry continues. Panel/session/OSD suppression follows PanelHost. Clear hover on hidden/destroyed surfaces and provider disconnect. Native inhibition is tested separately; it is not established by QML visuals alone. |
| History/DND | `modules/bar/popouts/NacreHistoryPopup.qml`, `modules/controlcenter/` notices, `services/NacreNotifs.qml`: newest first, loading/empty/error distinct, DND uses sole daemon settings writer, clear scope follows published contract. No mutation through old direct settings/history paths. |
| Persistence | Preserve Rust daemon's 30-day retention, existing version-1 formats and exclusions. Active retainable text restores as inert history, with no popup/action/deadline revival. Test transient/feedback exclusions, corrupt/readonly/future files, failed saves, clear-before-load tombstones and restart. Never replace this with Bead's current 256-row/3 MiB fixture store. |
| OSD | `modules/osd/NacreOsdEvents.qml`, `NacreOsd.qml`, `NacreLevelNotice.qml`: initial-state arming 400 ms, coalesced changes, focused output, bottom overlay, frame+24 px margin, width 200..336, no keyboard focus, 150 ms fade and 2000 ms hide. Hover pauses hide and leave restarts it. Volume/brightness use real published state; repeated equal-valued events use serial identity, expired/reconnected state never replays. Keyboard layout remains contract-blocked. |

## Implementation units after unblock

1. Pin the published protocol/producer. Add the concrete notifications/OSD
   consumer in `crates/nacre-shell/src/notifications.rs` and existing daemon
   client/runtime routing. Capability absence is unavailable/read-only; never
   start a local bus owner as reconnect fallback. Use the existing provider
   registration, call tickets, bounded state mirror and resync mechanisms.
2. Wire stack, history popout/control-center notices and OSD through the
   existing shell assets/runtime. Preserve stable IDs and local expansion;
   release lazy history/card/image resources after closed+idle. Resident service
   subscription and transient presentation lifecycle stay separate.
3. Exercise the published producer on private buses, then matched nested
   QML/Bead interactions. Update STATUS and this spec with evidence. Do not
   declare unit 5 complete with fixture-only screenshots or transport tests.

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
  on dev-1 via SSH/tmux: fmt, warnings-denied Clippy, workspace/trace tests,
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
- This documentation-only blocked stage creates no new implementation duplicate.

## Execution receipt

Spec and reference audit only. Product implementation, full CI and nested proof
are pending the producer/policy gate above; no runtime or dependency pin changed.
