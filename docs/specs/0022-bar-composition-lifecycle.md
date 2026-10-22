# B0022 — Per-output bar composition and lifecycle

Status: **Implemented — review; nested multi-output smoke recorded below**.
B0025 gives every output its own runtime/view state and Nacre N0031
([shell action routes](https://github.com/nacre-desktop/nacre/blob/main/docs/specs/0031-shell-action-routes.md),
pinned `652d6c4f`) publishes the semantic methods this bar calls. With a daemon
every state-changing bar action is one output-scoped N0031 call, usable only
while the connected daemon advertises that exact method at v1; otherwise the
control is disabled and nothing is sent. Without a daemon (preview/fixtures) the
older local paths remain; see Duplicates.

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

| QML action | Bead behavior (daemon mode) | Route / status |
| --- | --- | --- |
| App/launcher click; dashboard click | Local view actions (launcher trigger, `view.open`) | Views are Bead-local by N0031; no daemon mutation. |
| Workspace click | `workspace.activate {output, workspace: slot}`, slot = the button's `index` (1..255) from this output's row model | Done. Output is the session's compositor output name. Local `backend.workspace.switch` is not used in daemon mode. |
| Workspace scroll | `workspace.scroll {output, direction}`; wheel down = `next`, up = `prev` (QML `scroll`) | Done. One call in flight; no wheel accumulation (QML sums to 120). |
| Window focus | Not a bar action in QML | No bar use. |
| Active-title scroll | `audio.adjust_volume {delta: ±0.05}` | Done. Corrects the earlier table: QML `NacreActiveTitle` wheel adjusts volume, not workspace. |
| Clock click/hover | Calendar is a local view (`view.open calendar`) | Local by N0031. Hover peek stays as built; no daemon call. |
| Power click | Local session view; choice executes `session.request {action, confirmed}` | Done (B0023, schema fixed below). |
| Status audio wheel / right-click | `audio.adjust_volume {delta: ±0.05}` / `audio.toggle_mute {}` | Done. |
| Status battery wheel | `brightness.adjust {output, delta: ±5}` | Done; output-scoped. |
| Status network/Bluetooth right-click quick toggle | Existing B0013/B0014 controls in the status popouts | Unchanged; no new bar route. |
| Status click/hover | Local status pill + peek (B0018) | Unchanged. |
| Tray activate / secondary / context / scroll | `tray.activate`, `tray.secondary_activate`, `tray.context_menu {item,x,y}`, `tray.scroll {item,axis,delta:±120}` | Done. Items come from the daemon `tray` topic (visible rows, daemon icon file or theme name). `item_is_menu` items use `context_menu` on click. x/y are the item centre in the bar's logical space (single bar origin; output offset is not known to the session). No Bead D-Bus menu renderer in daemon mode. |
| Pointer exit/dismiss | Local transient-panel behavior | Local. |

Implementation: `crates/nacre-shell/src/bar_routes.rs` (route table, capability
gating, bounded pending tickets, tray topic rows) and `Runtime::with_bar_routes`
/ `bar_route` in `reload.rs`. Calls use `Client::begin_call` tickets guarded by
boot id and method version, so a transport change drops queued calls and nothing
is replayed. Relative and toggle calls (scroll, volume, brightness, mute) are
refused while the same method is in flight (the daemon also rejects concurrent
audio/brightness admission); at most 8 calls pend. `{changed:true}` is
acceptance, not readback. The `bar.workspace.enabled` / `bar.tray.enabled`
stores follow the welcome registry (re-evaluated on every link change) and
drop the button/tray actions in `shell/bar/main.luau`; wheel and right-click
routes are inert without their method. A daemon-mode event never falls back to
the local compositor, audio or tray path. The daemon's `tray` topic is added to
the UI client's topics.

The original blocker was architectural; B0025 cleared it (per-output sessions,
shared services and immutable resources). Workspace rows and the focused title
come from each session's own output-filtered model, and every call carries that
session's output name.

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

## Results (2026-10-22/23)

- Real-template runtime tests with a real `Client` against a fake daemon
  (`reload/bar_routes_runtime_tests.rs`, 3 tests): workspace/title/pill/tray
  calls and exact params, unadvertised methods disable controls and send
  nothing, daemon drop/restart disables then restores controls without replay.
  Unit tests cover params, tray-topic rows and the corrected `session.request`
  shape (`changed:true`, `confirmed:true`).
- Full Bead CI job (`.github/workflows/ci.yml` check, ubuntu:24.04 container on
  dev-2, Rust 1.99.0): fmt, clippy `-D warnings`, workspace and trace tests
  (shell 243 passed/1 ignored), feature checks, minimal example, Luau schema,
  five Python suites (54/11/12/10/8), service lifecycle, private SNI/DBusMenu
  lifecycle, cargo-deny graphs: all pass.
- Nested Hyprland, two private outputs, fake daemon, CPU renderer, owner
  desktop under `desktop-heavy.lock` (`BEAD_BAR_ROUTES_PROOF=1`, spikes/
  `bar-routes-proof.py` + `fake-bar-daemon.py`): `workspace.activate` arrives
  with `output` = the clicked output (`bead-bench` slot 1, `bead-extra` slot 2);
  tray rows from the topic produce `tray.activate` and (menu item)
  `tray.context_menu`; with the daemon killed both bars stay up with workspace
  and tray controls absent, a restarted daemon restores them and the earlier
  state is not replayed; a daemon advertising no methods again removes them;
  clean stop. Hotplug add/remove and scale change are the B0025 proof
  (unchanged code path; its run is not repeated here). Fake daemon, not a real
  Nacre daemon with N0031: real-daemon behavior is Nacre's N0031 evidence.
- Memory, relative gate (same host, same flags `--bar --core-views --renderer cpu
  --daemon-socket --ui-provider`, empty-tray fake daemon, nested Hyprland
  2880x1800 @1.5, CPU renderer, 3 runs each, control = `ff616d0` with its own
  shell tree): candidate − control median PSS one output +0.189 MiB, two outputs
  +0.234 MiB (anon +0.012 / +0.055 MiB); PASS (≤ +0.5 MiB, anon < 1 MiB). Tray
  icons are not in this comparison. Swap stayed 0 before and after every arm.
  `BEAD_PER_OUTPUT_DAEMON=1` adds these flags to `per-output-proof.py`.
- Open: x/y of tray anchors are bar-local; niri nested multi-output hotplug is
  still unsupported by the harness (B0025); the real-daemon end-to-end run
  needs a Nacre helper build with N0031 (not done here).

## Non-goals and stop conditions

No new tray, status-pill, or launcher feature beyond the wiring/lifecycle checks
above; no new daemon contract or Nacre-side change; no live service ownership,
desktop/config/install changes, package or publication. Stop and report if an
action needs an unpublished daemon method, if compositor behavior cannot be
kept behind existing adapters, or if the nested smoke/CI host is unavailable.

## Duplicates

The daemon-less local paths stay for preview/fixture runs and as the
rollback while the daemon bar is untrusted: `backend.workspace.switch` through
`Runtime::backend_action`, the `services.*` audio/tray actions through
`Runtime::desktop_action`, and the local tray bar/menu (`--tray` preview, B0016).
In daemon mode none of them is reachable. Expiry: delete 7 days after the
daemon-mode bar is trusted (same trust condition and expiry as N0031's QML
duplicate: stable Bead core-switch use and owner-approved retirement of the QML
shell).
