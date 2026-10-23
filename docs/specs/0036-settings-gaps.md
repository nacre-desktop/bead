# B0036 — Close the Bead-side settings gaps left by B0035

Status: **Review — panel enable flags, font families, text size and frame sheen implemented
live; workspace icons/count/names, window hints and the launcher rail not done (reasons below).**
Follows [B0035](0035-settings-stores.md). Nacre reference: `origin/main` at `bc03d708`
(`nacre/shell/**`). All values arrive through the existing `prefs.rs` follower of
`settings.store.desktop`, so they change live with no restart and need no new topic.

## Design (ADR 0004: Nacre keys only in `nacre-shell`; ADR 0005: memory)

Generic mechanisms, no Nacre key:

- `bead_ui::text_style` (MIT/Apache): a family alias table plus a size offset, consulted once
  per text view while the layout is built. Icon families (`Material Symbols*`) keep their size.
  `set()` reloads every view through the listeners the i18n catalog already uses (the same
  `UiEvent::Reload` path as a language switch), only when the style really changed.
- `bead_wayland::ShellControl` (MIT/Apache): `bar_enabled`, `panel_allowed(view)`,
  `decoration_variant`, plus `watch(callback)`. The loop re-evaluates on `ControlChanged`:
  bar surfaces are destroyed or recreated (panel surfaces are kept; a bar-off state is separate
  from the Super+Z hide), an open panel that became disallowed is closed, a changed decoration
  variant rebuilds the static frame tiles. `request_panel_at` refuses a disallowed open.

Nacre mapping (`nacre-shell`, GPL):

- `prefs.rs` decodes `dashboardEnabled`, `controlCenterEnabled`, `topBarEnabled`, `frameSheen`
  (QML `!== false`: only a real `false` disables), `interfaceFont`, `monoFont` (non-empty
  string, else default) and `textSize` (JS `value || 12`; the offset from 12 is added to every
  text size, as `NacreTokens.textSize`).
- `shell_control.rs`: `Gate` maps the flags onto `ShellControl`. Dashboard pages
  (`dashboard-*`) follow `dashboardEnabled`; `sound/audio/network/bluetooth/battery` and
  their hover peeks follow `controlCenterEnabled` (QML `openControls`); calendar, session,
  launcher and tray are not gated, as in QML. The bar's dashboard button is dropped through
  the host store `bar.dashboard.enabled` (`Runtime::with_prefs`).
- Fonts: Luau markup names `IBM Plex Sans` / `JetBrainsMono Nerd Font` literally; the alias
  table maps those two names to the owner's families. Default size offset 0 changes nothing
  (goldens untouched).
- `frameSheen`: `placement.rs` draws no rim path when off (QML sets the rim opacity to 0);
  open panels reload, the bar frame tiles rebuild through `decoration_variant`.

## Memory

A family switch changes the scene's font file set. `CpuRenderer`/`LayerTree::prepare` drop
the raster (and with it the old `FontSystem`) when `font_files` differs and the weak font
pool index holds no strong reference, so the previous face is released with the old scene. No
new cache. The bar-off state removes the bar's buffers. No matched capture was scheduled
(relative gate on the same host remains the check when a font switch is measured); estimated
new resident data: a few hundred bytes (style table, flags).

## Status of the B0035 G rows

| Item | Result |
| --- | --- |
| `dashboardEnabled`, `controlCenterEnabled` | **Done** gate in `request_panel_at`, open panel closed on change, dashboard button hidden |
| `topBarEnabled` | **Done** bar surfaces removed/recreated live; panels still open (QML hides only the header). `frameTopEnabled` only sizes the QML frame edge (`NacreFrame.headerHeight`); Bead has no desktop frame (A) |
| `interfaceFont`, `monoFont`, `textSize` | **Done** live reload; size offset applies to every non-icon text view |
| `frameSheen` | **Done** |
| `profile.workspaceIcons` | **Not done.** QML switches the whole capsule to an icon mode (46 px cells, accent fill, Material icon plus number). Needs a Luau icon-mode cell and a per-row `icon` projected at the three `backend_values` sites; no Nacre contract needed. Follow-up unit |
| `profile.workspaceCount` | **Not done.** QML uses it only for the non-per-output id range, the "named workspaces" cut-off and the tint position; Bead lists compositor workspaces and `bead_backends::Workspace` has no `fixedSlot`/`visiting` field to reproduce the cut-off. Needs those fields from the backend (not a Nacre contract) |
| `profile.workspaceNames` | Nothing to do: only `Overview.qml` shows names; Bead has no overview (A) |
| `workspaceWindowHints` | **Not done.** Bead's capsule shows no window icons yet (feature absent, not a settings gap): needs the active-cell app icon row and width allocation |
| `launcher-layout` | **Not done.** The launcher has no category rail; nothing to order or hide |

## Tests

- `bead-ui` `text_style`: alias/offset maths, icon exemption, change wakes listeners once.
- `prefs`: QML defaults and wrong types for every new key; wire test extended (fonts, text
  size and dashboard flag follow a live edit over the real client with a fake daemon).
- `shell_control`: flags gate their panels, watchers wake only on change, sheen bumps the variant.

## Temporary duplicates

None.
