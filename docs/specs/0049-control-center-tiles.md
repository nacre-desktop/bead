# B0049 - Control center: tile grid, detail pages, edit and catalog

Status: **Review** (Claude worker, branch `task/b0049-tile-editor-20261023`). Owner 2026-10-23: port every QML feature; Bead is a framework.
Reference: QML `controlcenter/NacreControl{Center,Surface,Overview,Detail,Tile,Edit,Catalog,AddOption}`, `NacreControlPreferences` (store `control-center`, key `tiles`).

## Structure

| Layer | Piece |
| --- | --- |
| Framework (kit) | `kit.ToggleTile/LinkTile/ButtonTile/SliderTile` (one tile kind each), `kit.TileGrid` (packs by size), `kit.ReorderList` (drag reorder). Generic: values and ready actions in, no Nacre ids. |
| Engine | `bead.pressed` store plus `UiEvent::Release`/`Session::release`: a press action sees `false`, drag steps see `true`. This is what lets Luau tell a new drag from a continuing one. |
| Data + view (Luau) | `shell/control/tiles.luau` = the registry (`{id, kind, size, icon, label, source, view/page, default, ask}`); `main.luau` = home, detail, edit, catalog pages; `behavior.luau` = draft and drag actions. A tile over an existing source or panel needs no Rust. |
| Adapter (GPL) | `nacre-shell/src/control_center.rs`: the only file naming topics/methods/the store. Projects `cc.src` (value, state, offered verbs), `cc.saved`, `cc.views`; runs `cc.do` (`toggle|set:<source>[:pct]`); writes the draft. |

## Behaviour

- Open: `drawers toggle right` toggles, `nacre controls home|edit` opens the `control` panel (right edge, 424 wide, tall as its content, scrolls past the output).
- Tiles: only those saved in `control-center.tiles` (else the registry defaults) whose source/panel is published. Unavailable tiles never show; the catalog lists them under "Waiting for Nacre" with their ask, so they appear when the contract exists.
- Link/detail: network, Bluetooth, power mode (battery), clipboard, updates (dashboard system) and notifications open the existing Bead panel with `view.open`; Do Not Disturb has an in-panel detail page.
- Edit: drag a row over another to move it, X removes, "Add controls" opens the catalog, Reset restores defaults. Every change is persisted at once.
- Persistence: `settings.set {store:"control-center", key:["tiles"], value, rev}`, one call at a time with the newest draft, revision from the last reply or the leased `settings.store.control-center` topic; `stale_revision` retries once, other refusals show a message and are not retried until the draft changes. Not advertised or store unreadable: editor stays visible, nothing is written, a note says so. B0045's writer had not landed; this one is 60 lines in the adapter. Merge into one writer when B0045 lands.
- Closed: nothing resident; topic leases drop with the panel.

## Sources and Nacre asks

Published and used: `audio` (volume via `audio.adjust_volume`, mute via `audio.toggle_mute`), `network` (`network.set_wifi`), `bluetooth` (`bluetooth.set_enabled`), `power.profiles` (read), `notifications` (`notifications.set_dnd`), `battery` (header).

| Tile | Ask |
| --- | --- |
| brightness, keyboard backlight | `brightness` / keyboard topics with level + set |
| mic volume, mic mute | `audio.set_volume`, `audio.set_mute` advertised (also: state whether `volume` is linear or perceptual) |
| airplane, VPN, night light, stay awake, recorder, screenshot, picker, displays, appearance | topic + action per feature |
| wallpaper, next wallpaper | settings window route (B0045) / wallpaper advance method |
| power profile switch | `power.set_profile` |
| Volume slider | absolute set; today relative steps of at most 0.2 per call until `audio.set_volume` exists |
| i18n | 45 `bead.panel.control-*` ids; run `tools/i18n-sync.py NACRE --write` |

## Parity left

Details open the existing panel (no in-panel back button there); header Settings button (needs the window route); tile long-press/context details; notification list inside the sidebar; detached/compact presentations.

## Generalization list additions

`kit.ToggleTile/LinkTile/ButtonTile/SliderTile/TileGrid/ReorderList` are general. The kit still uses Nacre palette roles (existing entry). Adapter table of sources is Nacre-specific by design (edge).

## Checks

Fake-daemon tests (calls, persistence, refusal, stale retry, unpublished route), Luau drag/dispatch tests, eight goldens (`control-{overview,detail,edit,catalog}-{1,1.5}.png`, eyeballed). Full CI equivalent passed on prod (fmt, clippy -D warnings, 340 nacre-shell tests, workspace tests, renderer configs, facade/Luau check, py, deny). No nested smoke or memory capture run (CPU renderer; closed panel holds nothing).

## Temporary duplicates

None.
