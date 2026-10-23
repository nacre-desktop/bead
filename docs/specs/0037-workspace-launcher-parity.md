# B0037 — Workspace icons and count, window hints and the launcher rail order

Status: **Review — implemented and tested; the three Nacre blockers below were resolved by N0040
and consumed in [B0038](0038-rail-visiting-focus.md).** Follows [B0036](0036-settings-gaps.md). Nacre reference: `origin/main`
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
| `launcher-layout` `order`/`hidden` rail | Luau `launcher.navigation` ranks `home`, the order, then the default order; hidden ids drop out (never Home) (B0038: over the engine's `menu.groups`) |

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

## Blockers recorded (precise Nacre contracts) — all three resolved by Nacre N0040, consumed in [B0038](0038-rail-visiting-focus.md)

| # | Blocker | Nacre contract (N0040, `4d816ca2`) | Bead |
| --- | --- | --- | --- |
| 1 | Engine category rail: `menu.query` carried no rail groups, B0019 forbids inventing a taxonomy | `menu.groups {}` -> `{catalog_rev, groups:[{id,label,icon}]}`, nine groups in default order, each id a `menu.query` route | Rail drawn from `menu.groups`; the four-entry rail is gone (B0038) |
| 2 | `visiting` workspaces and `originOutput/originIndex` computed from a ledger Bead cannot read | niri `compositor` topic workspace rows gain optional `originOutput`, `originIndex`, `visiting`; absent on Hyprland or without a ledger entry | Visitor groups drawn from the topic (B0038) |
| 3 | Click on a window glyph focuses the window; no published route | `window.focus {window, output?}`, output now optional | Glyphs focus their window, disabled when the method is not advertised (B0038) |

## Deviations and limits (one line each)

- Window glyph lookup tries `<app_id>.desktop` and its lower-case form; Quickshell's
  `heuristicLookup` also matches `StartupWMClass`/reverse-domain suffixes. Misses show `apps`.
- Absent workspaces are not synthesised for non-per-output backends (Hyprland lists only existing
  ones); switching to a missing id needs an index-based action.
- ~~Default rail order keeps Bead's existing order (apps first)~~: superseded, B0038 uses QML's order.
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
