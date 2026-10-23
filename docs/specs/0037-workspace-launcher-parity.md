# B0037 — Workspace icons and count, window hints and the launcher rail order

Status: **Review — implemented and tested; the engine-published category rail (style, setup,
software, health, learn, power) and click-to-focus on window hints are blocked on Nacre
contracts (below).** Follows [B0036](0036-settings-gaps.md). Nacre reference: `origin/main`
(`nacre/shell/modules/topbar/NacreWorkspaceRow.qml`, `NacreWorkspaceAppIcon.qml`,
`launcher/menu-model.js` `rail()`). Values arrive through the B0035 `prefs.rs` follower, so
they change live; it now also follows `settings.store.profile` and
`settings.store.launcher-layout` (absent topics keep Nacre's defaults).

## What the QML does, and what Bead does now

| QML | Bead |
| --- | --- |
| `profile.workspaceCount` (`\|\| 6`): non-per-output id range; per-output "named workspaces" cut-off | `workspace_rows.rs` (GPL) lists rows exactly so: per-output = own non-visiting rows, only named/occupied ones once the named slots reach the count, plus occupied visitors; global = `1..=count` plus active and occupied (`>= 1`) |
| `profile.workspaceIcons`: any non-empty entry switches the capsule to icon mode (46 px cells, accent fill when active, symbol and number, dot when occupied) | Row fields `icon`, `icon_mode`, `occupied`; `shell/bar/main.luau` draws the icon cell |
| `workspaceWindowHints !== false`: active workspace shows up to three window glyphs, `+N`, width allocated from the bar width | Same allocation (cell 32, glyph 18, overflow 20, `+4`; available width `max(170, min(720, W/2-130, W-330-220))` on the output's logical width); row fields `shown`, `overflow`, `hinted`, `app1..3` |
| Window glyph = `NacreWorkspaceAppIcon`: Material symbol by desktop-entry `Categories` (not the app's image icon) | Same table and order; `Categories=` read from `<app_id>.desktop` (exact, then lower case) in the XDG data dirs, `nacre-*` names fixed |
| `launcher-layout` `order`/`hidden` rail | Luau `launcher.navigation` ranks `home`, the order, then `apps`/`actions`; hidden ids drop out (never Home); unranked rows (Bead's `search`) go last |

Backend: `bead_backends::Workspace` gains `fixed_slot` (niri: the workspace has a name) and
`visiting` (always false, see blockers); both `#[serde(default)]`. QML's "named" test is
`fixedSlot || non-numeric name`, so niri named workspaces behave identically. The workspace
number shown is the compositor `index` (previously the `name`, which was the niri name for named
workspaces).

Plumbing (no new topic): `Prefs` gains `workspace_count/icons`, `window_hints`, `rail_order/hidden`;
`bar_stores()` adds `bar.workspace.count/icons/hints`, `launcher_stores()` adds
`launcher.rail.order/hidden`. `Runtime` keeps a `workspace_rows::Rows` (view + glyph cache) used by
the three `backend_values` call sites and re-projects `bead.workspaces` when a `bar.workspace.*`
store arrives or `with_prefs` runs. The launcher panel runtime now also follows the prefs source.

## Blockers recorded (precise Nacre contracts)

1. **Engine category rail.** `menu.query` results carry items/order only; the rail groups
   (`home, apps, actions, style, setup, software, health, learn, power`, their labels and icons)
   live in the QML client's catalogue (`launcher-menu.json` via `NacreMenuContext`), not in any
   published method, and B0019 forbids inventing a taxonomy. Needed: a `menu.groups` method (or a
   `groups` field on `menu.query`) with `{id,label,icon}` per top-level group. Until then the rail
   keeps Bead's four entries (apps, favourites, search, actions) and only applies order/hidden.
2. **`visiting` workspaces and `originOutput/originIndex`.** Nacre computes them in the helper
   (`niri_workspace_policy`) from a ledger Bead cannot read; niri itself has no notion. Needed:
   the compositor topic's workspace rows (`fixedSlot, visiting, originOutput, originIndex`) as a
   source for the bar, or the same fields in the niri adapter's input. Bead reports
   `visiting=false`, so visitor groups and origin labels are not drawn.
3. **Click on a window glyph focuses the window** (QML `focusWindow`). The daemon bar routes
   (`bar_routes.rs`) publish `workspace.activate/scroll`, volume, brightness and tray only; there
   is no `window.focus` route, and bar actions must not fall back to the local compositor. Needed:
   a published `window.focus {output, window}` route. The glyphs are display-only meanwhile.

## Deviations and limits (one line each)

- Window glyph lookup tries `<app_id>.desktop` and its lower-case form; Quickshell's
  `heuristicLookup` also matches `StartupWMClass`/reverse-domain suffixes. Misses show `apps`.
- Absent workspaces are not synthesised for non-per-output backends (Hyprland lists only existing
  ones); switching to a missing id needs an index-based action.
- Default rail order (no `order` configured) keeps Bead's existing order (apps first) instead of
  QML's Home first, to leave the accepted launcher goldens alone.
- The active workspace's tinted capsule is drawn only when hints are shown; the accent cycle
  (`index % 3`) is unchanged from B0022 rather than QML's count-based gradient position.
- `RIGHT = 330` approximates the right cluster width QML subtracts from the hint width.

## Memory (ADR 0005)

No image is decoded: hints are font glyphs (the Material face the bar already loads). The only new
state is the glyph cache, capped at 64 `(app id, &'static str)` entries and cleared when full, and
four small store values (count, icon strings capped at 32 x 120 bytes, flag). The old
"icons decoded at display size and released like tray pixmaps" design is not needed. No matched
capture was run; the relative gate (candidate - control on the same host and backend) is the check
if a measurement is scheduled.

## Tests

- `workspace_rows`: store follower, per-output cut-off and visitors, global ids, icon mode, hint
  allocation and `+N`, QML width rule, glyph table, desktop-entry reader and cache bound.
- `prefs`: QML defaults and wrong types for every new key, bounds, live profile/layout follow.
- `desktop`: the real bar bundle draws 32/46 px cells and the hint glyphs from row fields.
- `launcher_pixels_tests`: rail order, hidden ids, Home never hidden.

## Temporary duplicates

None.
