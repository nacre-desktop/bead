# B0022 — Per-output bar composition and lifecycle

Status: **Partial; protocol contracts and B0023 memory/proof acceptance remain blocked**. B0016 tray,
B0018 status pill, and B0019 launcher are existing work and are not reopened as
independent features here. The QML reference is Nacre `origin/main` at the task
start. The Wayland runtime already creates one primary layer surface per
announced output, updates it on output geometry/scale changes, and removes it
on output destruction. The local [B0023 candidate](0023-per-output-ui-sessions.md)
now gives each output its own runtime, view state and scoped backend projection.
Its full CI passed, but the two-output memory screen reached 26.043 MiB and
triggered the standing 24.5 MiB stop. Remaining private lifecycle/input proofs
are incomplete; that architecture acceptance and the published action contracts
still block this consumer. No source from B0023 has been pushed.

Output surface lifecycle is already implemented in
`crates/bead-wayland/src/runtime.rs` and `protocols.rs`: output announcements
create the primary surface, geometry updates reuse it, preferred scale redraws
that surface, and output destruction removes its surface. Daemon transport
reconnect already takes a fresh snapshot and fences queued calls by transport
generation (`nacre_source` tests); these mechanisms do not yet prove a full bar
survives the daemon restart in a nested multi-output session. Tray bus-loss
recovery and item replacement are covered by the existing ignored supervised
private-bus test `session_bus_eof_reconnect_supervised` in
`crates/bead-services/tests/tray_private_bus.rs`.

## Objective and scope

Complete the always-on Bead bar on both Hyprland and niri: create exactly one
bar for every active output; compose the published workspace, active-window
title, clock, power, tray, status-pill and launcher entry points; and keep each
bar associated with its own output through hotplug, geometry/scale updates,
daemon disconnect/restart and tray owner/item replacement. The bar must remain
visible and usable while daemon state is unavailable, then resubscribe and
refresh from a new snapshot without replaying stale actions. Output removal
releases only that output's bar and view state.

Match the Nacre QML bar's supported click, scroll, hover/peek and dismissal
behavior as an action inventory, but route state-changing actions only through
published `nacre-protocol` methods. No guessed method names, shell commands,
direct D-Bus mutations, or Bead-owned ranking/providers. If QML behavior has no
published route, record it as a contract blocker; do not silently substitute.
Keep compositor specifics behind existing adapters. No Nacre branding,
palette, labels, iconography or other Nacre-specific appearance may enter any
`bead-*` crate. Retain the shared palette/component contracts already used by
Bead.

## Reference inventory

Compare current `nacre/shell/modules/topbar/NacreHeader.qml`,
`NacreWorkspaceRow.qml`, `NacreStatusPeek.qml`,
`nacre/shell/modules/bar/components/NacreActiveTitle.qml`,
`NacrePowerButton.qml`, and `NacreStatusIcons.qml` on fetched Nacre
`origin/main`. Cover: app/launcher and dashboard entry; output-scoped workspace
activation, workspace scrolling and window focus; active-title scrolling;
clock/calendar entry and hover; power/session entry; status click, scroll,
hover peek and popup handoff; tray item activate/context/menu actions and hover
tooltip; and pointer exit/dismissal. B0018/B0019/B0016 own the implementation
for their already completed entry points; this unit verifies their bar wiring
and lifecycle without duplicating those features.

| QML action | Bead behavior | Protocol status at spec time |
| --- | --- | --- |
| App/launcher click; dashboard click | Existing launcher trigger and local `view.open` action | `ui.dispatch` v1 is published in pinned `state.jsonl` / `ui.jsonl` and Bead's provider forwards validated calls through it. The bar's current launcher/dashboard clicks are local panel actions, not daemon-backed; no published per-output route is established. |
| Workspace activate, scroll, window focus | B0023 candidate scopes state by output; workspace click still uses local `Backend::action` | Blocked: `nacre.workspace(int)` exists only in Nacre's shell command catalog, not the pinned daemon method registry. `backend.workspace.switch` is a direct compositor mutation and is disallowed for this parity route. No focus-window daemon method is published. |
| Active-title scroll | No matching behavior wired in Bead bar | Blocked: QML maps title scroll to adjacent workspace; same missing published workspace route and output context. |
| Clock click/hover | Calendar view opens via Bead UI action; no hover peek parity | `ui.dispatch` v1 is generic, but `nacre.popout` is shell catalog only. No published calendar-open/peek daemon method or output argument. |
| Power click | No action currently wired on Bead bar | `nacre.session` is shell catalog only. No published session-entry daemon method. |
| Status click/scroll/hover | Existing status pill and detail views | `ui.dispatch` v1 exists; specific `nacre.controls/popout` names are shell catalog only. No published status hover/scroll or output-scoped action contract. Existing B0018 behavior remains separately implemented. |
| Tray activate/context/menu/scroll/hover | Existing B0016 tray service and tooltip; actions use local tray service | Pinned v1 publishes `tray` state topic but no tray action methods. `ui.dispatch` does not define tray action payloads. Existing private-bus test covers loss, empty snapshot, replacement item and activation; compositor bar restart/handoff smoke remains open. |
| Pointer exit/dismiss | Local transient-panel behavior | Local view behavior; no daemon mutation required. Pinned panel persistence/hover handoff still needs parity proof. |

The generic `ui.dispatch` method is published at protocol v1, but the route
names above are not daemon methods merely because they appear in Nacre's shell
command catalog. Do not wire those names as protocol calls without a published
contract. The pinned registry also exposes `daemon.ping` and `daemon.status`;
neither represents any of the blocked bar actions.

The original blocker was architectural: adding model fields could not repair
one shared `SceneSource`, committed scene, primary UI serial and pointer target.
B0023 implements per-output sessions, update/commit/input/teardown routing and
late-name handling while keeping services and immutable resources shared.
Its memory stop and incomplete private proofs now replace the missing runtime
implementation as the architecture blocker; see its measured result before
resuming this consumer. This does not publish any missing daemon action route.

## Acceptance

- A matrix from every supported QML bar click/scroll/hover action to either an
  existing published daemon method or a documented unresolved protocol blocker
  exists here; all implemented mutation routes use published method/type and
  transcript versions. No direct mutation outside the protocol.
- Nested niri and Hyprland smoke with two virtual outputs proves one correctly
  scoped bar per output; workspace/title/status state is output-correct; add,
  remove and scale/geometry changes update only affected bars; tray registration,
  owner loss/replacement and item removal are safe.
- Inject daemon stop/restart/disconnect and protocol resubscription. Bars stay
  visible; stale calls are not replayed; a fresh snapshot restores current
  state; tray/pill/launcher entry points remain available and recover.
- Focused tests cover output enumeration/update/removal, reconnect sequence,
  stale action fencing, and tray-item lifecycle. No claim of live or physical
  scanout validation.
- Run the full Bead desktop CI job in the isolated Arch CI container on prod,
  then run the private nested multi-output smoke on the owner's desktop while
  holding `~/nacre-lead/desktop-heavy.lock`. Never test on the laptop or daily
  shell. Report exact commands and results.
- Keep changes minimal and report added/removed product lines. Any temporary
  duplicate path must be listed with expiry: delete N days after the replacement
  is trusted. Update `docs/STATUS.md` and this queue at the milestone.

## Non-goals and stop conditions

No new tray, status-pill, or launcher feature beyond the wiring/lifecycle checks
above; no new daemon contract or Nacre-side change; no live service ownership,
desktop/config/install changes, package or publication. Stop and report if an
action needs an unpublished daemon method, if compositor behavior cannot be
kept behind existing adapters, or if the nested smoke/CI host is unavailable.

## Duplicates

There are no planned temporary duplicates. If implementation requires one, add
its path and explicit expiry (“delete N days after the replacement is trusted”)
here before introducing it.
