# B0022 — Per-output bar composition and lifecycle

Status: **Spec first; implementation pending**. B0016 tray, B0018 status pill,
and B0019 launcher are existing work and are not reopened as independent
features here. The QML reference is Nacre `origin/main` at the task start.

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
| App/launcher and dashboard entry | Existing launcher/dashboard entry | Verify against the pinned UI provider method registry |
| Workspace activate, scroll, and window focus | Output-scoped workspace switch/focus | No daemon route found in current Bead v1 fixtures; contract gate before wiring |
| Active-title scroll | Previous/next workspace | No daemon route found in current Bead v1 fixtures; contract gate before wiring |
| Clock click/hover | Calendar open/peek | Verify pinned UI provider method registry |
| Power click | Session/power entry | Verify pinned UI provider method registry; no guessed local command |
| Status click/scroll/hover | Existing pill/status routes | Verify pinned method per action and target output |
| Tray activate/context/scroll/hover | Existing item/menu/tooltip routes | Verify published tray methods and reject stale item identity |
| Pointer exit/dismiss | Close transient peek, preserve pinned panel | Local view behavior; no daemon mutation |

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
