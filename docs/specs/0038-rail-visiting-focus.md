# B0038 — Engine rail, visiting workspace groups and window-glyph focus

Status: **Review** (Claude worker, branch `task/b0038-rail-visiting-focus-20261023`). Consumes Nacre N0040
(`4d816ca2`, unpublished; [spec](https://github.com/nacre-desktop/nacre/blob/main/docs/specs/0040-groups-visiting-focus-contracts.md))
and closes the three blockers of [B0037](0037-workspace-launcher-parity.md). The `nacre-protocol` pin moves from
`3499bcbe` to `4d816ca2`. Reference: `menu-model.js` `rail()`, `NacreAppBrowser.qml` (rail), `NacreWorkspaceRow.qml`.

## Behavior

| Item | Behavior |
| --- | --- |
| Rail | The sidebar lists `menu.groups` (home, apps, actions, style, setup, software, health, learn, power; labels and icons from the engine, localized). Order and hiding are QML's `rail()`: Home, the `launcher-layout` order, then the default order; hidden ids drop out, never Home. Active entry = first segment of the engine route, else the page (`apps`, `actions`). Entry look (37 px, 3 px gap, accent bar, 20 px icon, 14 px label) is unchanged from the QML match. |
| Rail navigation | `home` and the engine groups (`style` ... `power`) query `menu.query` with that route like Home did (grid of rows); `apps` and `actions` keep their existing page/prefix behavior. The page title is the group label. Bead's own `search` entry is gone (the QML has none). |
| Without `menu.groups` | Older daemon, fixture without `groups`, or before the first reply: the rail shows `home`, `apps`, `actions` (the groups every engine has) and switches to the engine rail when the reply arrives. Nothing is invented. |
| Visitor groups | niri only. The bar leases the daemon `compositor` topic and keeps each workspace's `originOutput`/`originIndex`/`visiting` (missing `visiting` = false). Occupied visiting workspaces follow the local ones, sorted by home output then home number; each group starts with a 48 px space holding a rule and `▣ <home output>`; the cell shows the home number (also used for `workspaceIcons`). Hyprland, or no ledger entry: nothing changes. |
| Glyph focus | Each hint glyph is a transparent button over its glyph (`focus1..3`, sibling of the workspace button) calling `window.focus {window}` with the daemon's window id (`niri-window:<n>`, or the Hyprland address) and no output. Disabled (glyphs display-only, no button) while the daemon does not advertise `window.focus` v1; never falls back to the local compositor in daemon mode. |

## Implementation

- `launcher.rs`: `menu.groups` is fetched by the existing query actor whenever the source stamp changes (same trigger as
  `menu.preferences`); a bounded `Vec` of `{id,name,icon}` rows is projected into the `launcher.groups` store. Fixture source
  answers `menu.groups` from an optional `groups` key.
- `shell/ui/behavior.luau`: `launcher.navigation` ranks the groups (QML `rail()`); `navigate` sets the engine route for any
  group other than `apps`/`actions`.
- `workspace_rows.rs`: `View.origins` follows the `bar.workspace.origins` store (rows from `origin_rows`, at most 64);
  rows gain `visiting`, `origin`, `group_start`, `win1..3` (local ids); labels use the home number.
- `reload.rs`: `with_workspace_origins` (niri only) leases `compositor` and publishes `bar.workspace.origins` only when the
  origin rows change; `backend.window.focus` joins the daemon route intercept. `bar_routes.rs`: `Route::WindowFocus`
  and the `bar.window.focus.enabled` capability store.
- `shell/bar/main.luau`: each workspace is a `cell` box (group rule/label, workspace button, glyph overlays).
  i18n: `bead.bar.focus-window` added, `bead.launcher.search` removed (unused).
- Goldens: only the six launcher goldens changed, and only in the rail (nine entries, Home first); regenerated with
  `BEAD_UPDATE_GOLDENS=1` and compared by eye.

## Deviations and limits (one line each)

- The rail has no scroll: nine 37 px entries need about 357 px, which fits the 590 px panel; a panel clamped below ~480 px
  clips the last entries (the QML list scrolls). Not addressed.
- Groups are fetched per launcher open (the panel's `Menu` is dropped on idle close and view state is deliberately scalar-only), so the
  first frame can show the three-entry fallback rail for one round trip.
- `catalog_rev` is not compared; the rail refetches on the source stamp change, which the daemon bumps with the catalogue.
- The glyph tooltip and the accent bar gradient of the QML rail are not drawn (unchanged gaps).
- Hint allocation ignores group space, as the QML does.

## Memory (ADR 0005)

No new process or session (ADR 0005): the bar already runs in the one UI process; the only additions are a lease on the
`compositor` topic (latest snapshot held by the client, a few KB) on niri bars, at most 64 origin rows, nine group rows in
the launcher, and one overlay button node per visible glyph. No image is decoded. The relative gate (candidate - control,
same host and backend) was not run: the owner's desktop nested session was held by another worker's lock
(`~/nacre-lead/desktop-heavy.lock`), so no nested smoke or capture was taken; remote CI covers the behavior tests.

## Licensing (ADR 0004)

All code is in GPL `nacre-shell` and the GPL `shell/` assets; no framework crate (`bead-*`) changed.

## Tests

- `workspace_rows`: topic origin rows, visitor ordering by home output and number, group start, missing fields, id mapping.
- `launcher_pixels_tests`: nine-group rail order and hidden ids, Home never hidden, the three-entry fallback.
- `bar_routes_runtime_tests`: a glyph click sends `window.focus {window}` with no output; no glyph button when the method
  is not advertised; the leased `compositor` topic reaches `bar.workspace.origins`.
- `desktop`: the real bar bundle draws the group label and keeps the 32 px cell.

## Temporary duplicates

None.
