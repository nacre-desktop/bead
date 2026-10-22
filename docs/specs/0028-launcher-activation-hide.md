# B0028 — Launcher row activation and shell hide state

Status: **Review — implemented; nested smoke of hide and of a live daemon activation pending.**
Closes B0026 blockers D (partly) and A. Nacre reference: `origin/main` at `dedef897`;
protocol pin unchanged (`294d3f40`, `menu-ui-actions.jsonl` verified identical to
Nacre `protocol/fixtures`).

## D. Launcher row activation (`ui.menu.activate`)

Flow (published contract). The launcher already sends `menu.activate {id, activation,
catalog_rev}` with the row's ticket (B0019: it refuses rows from a stale result, a
changed epoch/revision or a disconnected link before sending). For every row the
engine does not run itself, the daemon sends the selected row to the registered `ui`
provider as `ui.menu.activate {row, catalog_rev}`, waits for that reply, then finishes
the outer call. The provider now registers `ui.dispatch:1` and `ui.menu.activate:1`
(exactly the registration frame of `menu-ui-actions.jsonl`).

The handler validates the typed row (`MenuUiActivate::validate`, disabled rows refused),
chooses the action, **acknowledges with `{keep_open}`, and only then executes**. Execution
never waits on the launcher, whose worker is blocked inside the outer `menu.activate`
(the protocol's no-deadlock rule). Failures after the acknowledgement are logged
(`UI_MENU_ACTION_FAILED`); the launcher cannot be told.

| Row operation | Bead action | `keep_open` | QML equivalent |
| --- | --- | --- | --- |
| `copy` (literal text) | `wl-copy` with the text on stdin, no shell | false | `wl-copy -- value` |
| `copy-clip` (entry id) | daemon `clipboard.copy {id}` | false | helper `clipboard.copy` |
| `command` (argv, directory) | spawn argv literally, own process group, no shell | false | `AppLaunch.run` |
| `power` `lock` | daemon `session.request {action:"lock"}`; success needs `locked:true` | false | `loginctl lock-session` |
| `power` logout/suspend/reboot/poweroff | open Bead's session view, which owns the confirmation | false | confirm dialog, then run |
| `route` | the `nacre menu ROUTE ''` key route (aliases resolved, `wallpaper` opens the wallpaper route) | true | `navigate(route)` |
| anything else (`tool`, `wallpaper`, `clip-pin`, `form`, ...) | refused `unavailable`, "Launcher action is not supported by this UI" | n/a | needs views Bead lacks |

The row is trusted only because the daemon sends it after a ticket check; direct client
calls are refused by the daemon. No row text reaches a shell. A power action other
than lock never runs from the launcher: the `confirm` prompt is not a confirmation.

Stale rows. Local fences from B0019 are unchanged. New: a daemon `stale_revision`
reply to `menu.activate` now clears the view's request and result stamp, so the next
projection queries again (QML `activate` -> `schedule()`); the error text is kept.
After a successful activation the launcher closes unless `keep_open`, as before
(`activation_changes` -> `panel.open`).

Not done (needs Nacre contracts or views Bead lacks): `wallpaper`/`next-wallpaper` rows
(no published setter; B0021 wallpaper is daemon-driven), recording/screenshot `tool`
rows, saved workflows, settings forms, clipboard pin/delete. Rows with these operations
stay visible and fail with the published refusal.

## A. Shell hide

QML reference (`NacrePanelState.hidden`, `NacreShellIpc.hide`, `NacreScreen`,
`NacreReservedEdges`, `NacrePanelInput`, `usage.md`). `nacre hide` **toggles** one
shell-wide flag. While hidden: the header/frame, lips and every panel are invisible, the
four border reservations drop to 0 (tiled windows gain the space), input regions are
empty, keyboard focus is none, OSD/level notices and Caps Lock polling are suppressed.
Wallpaper is untouched. Panel and view state is **kept**: hide does not close anything.
Any explicit open (`openMenu`, `openMode`, `openEdge`, `openControls`, `toggleLeft`,
dashboard toggle) sets `hidden=false`. Fullscreen is independent: QML's own
fullscreen hiding (`visible: !fullscreenFor(output) || modal`) is separate and Bead has
no such handling at all yet (gap, unchanged).

Bead (`bead-wayland/src/hidden.rs`, generic):
- `App.hidden`; `nacre hide` maps to `bead.panel hide` (toggle), replacing the old
  panel-close mapping. `nacre close` still closes the panel.
- Hiding destroys every bar and panel layer surface and the dismiss catcher; the
  compositor releases the exclusive zone, input region and keyboard focus with them.
  (A merely unmapped layer surface is reset to its initial state and must be given
  its anchors and size again; first nested run confirmed the compositor rejects a
  bare remap with a protocol error, so show goes through `add_output`, the start-up
  and hot-plug path, which owns that geometry.) Wallpaper surfaces are separate and
  not touched. Panel runtime, view and model stay alive, and a panel that was open
  comes back with its surface.
- Showing re-creates the dismiss catcher when a keyboard panel is open and then the
  surfaces of every output. Outputs added while hidden create nothing until shown.
- `draw` returns early while hidden, so no stray redraw can remap a surface.
- Explicit opens clear hidden first: `bead.panel open|toggle`, the launcher-family key
  routes and the dashboard toggle (`UiEvent::ShellHidden(Some(false))`). Automatic
  opens do not: OSD feedback and status peeks (`CompanionVisibility(true)`,
  `CompanionSelect`) are dropped while hidden, as QML suppresses its OSD.
- `nacre state` reports the real `hidden` value (was constant false); `bead.panel state`
  has it too.

Differences from QML, accepted: no 200 ms opacity fade (unmap is immediate); a
notification that arrives while hidden is not re-shown on unhide (it stays in history).

## Out of scope (recorded)

- Brightness OSD value after `brightness.adjust`: needs a Nacre readback contract
  (B0026 blocker E). Keyboard light: no published method (blocker B). AI sidebar (C).
- Fullscreen hiding of the bar (existing gap, not part of this unit).

## Tests

- `ui_provider::tests`: every published row of `menu-ui-actions.jsonl` (copy, power
  restart, copy-clip, literal-argv command, unknown operation with the published refusal),
  lock, route aliases and fail-closed malformed rows.
- `nacre_source` test `actual_provider_acks_published_rows_and_refuses_unknown_operations`
  replays transcript rows 15-18 through the real provider; the two existing
  actual-provider ack tests now expect the two-method registration.
- `launcher` test `daemon_stale_revision_drops_the_rows_and_queries_again_like_qml`.
- `commands` test: `nacre hide` maps to `bead.panel hide` on every command map.
- Compositor-level hide/show needs a nested session (see Acceptance).

## Temporary duplicates

None added.

## Acceptance

Bead CI-equivalent on a remote host (fmt, clippy `-D warnings`, workspace tests,
feature checks, Luau, services, deny). Nested smoke (follow-up on the desktop): hide
unmaps the bar and an open panel, a second hide restores both and the reserved space,
launcher key while hidden restores; with a daemon, a `copy` row reaches `wl-copy`.
