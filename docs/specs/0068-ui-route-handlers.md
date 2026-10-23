# B0068 - every Nacre `ui.dispatch` route answered by Bead; apps follow the live palette

Status: **Review** (Claude worker, branch `task/b0068-route-handlers-20261023`). Consumes Nacre N0048 (`fdc351ec`):
every `nacre CMD`, generated bind and app launcher is `nacrectl ui TARGET METHOD ARGS`, which the daemon forwards as
`ui.dispatch` to the registered UI provider (Bead's `ui_provider.rs` -> command socket -> `commands::map_views` and the
shell runtime). N0048 listed the routes Bead did not answer; this unit answers every one or states why not.

## Coverage table

Source: `shell/shell-routes.json`, a snapshot of Nacre `nacre/shell-tools/shell-routes.json` at `fdc351ec`
(re-copy it when Nacre adds a route; the test below then names the missing row).

| Route | Bead handler | Status |
| --- | --- | --- |
| `nacre launcher [Q]` | `key_routes::launcher` (toggle) | done (B0026) |
| `nacre menu ROUTE Q`, `nacre clipboard` | `key_routes::launcher` | done (B0026) |
| `nacre mode apps\|wallpaper\|palette\|overview\|clipboard\|keys` | `key_routes::launcher`: daemon menu routes `style.wallpapers`, `style`, `actions.windows`, ... | done for the menu-backed modes. **Gap:** QML's overview grid and palette picker are not rebuilt; `overview` lists windows in the launcher (`actions.windows`), `palette` opens the launcher Style page |
| `nacre hide`, `nacre close` | `bead.panel hide` / `close` | done (B0028, B0026) |
| `nacre settings [PAGE]` | Settings app (`app::route`) | done (B0045, B0052) |
| `nacre left`, `drawers toggle left` | chat app (`app::route`) | done (B0052) |
| `nacre controls home\|edit`, `drawers toggle right` | control centre | done (B0049) |
| `nacre controls calendar\|network\|bluetooth\|battery\|sound\|audio` | their panels | done (B0049) |
| `nacre controls session` | `bead.panel open session` | **new** (B0068) |
| `nacre controls notifications` | `bead.panel open dashboard-notifications` (the notification history page; QML shows a section of the right sidebar) | **new**, nearest equivalent |
| `nacre controls updates` | refused `unavailable` ("no updates view") | **unsupported**: Bead has no updates view; Nacre's updater is a launcher/Settings flow |
| `nacre session`, `drawers toggle session` | `bead.panel toggle session` | done (mapping existed; the test now guards it) |
| `drawers toggle dashboard\|sound` | dashboard / sound panel | done (B0026, B0049) |
| `drawers toggle launcher` | `nacre launcher ""` | **new** |
| `drawers toggle osd\|leftPinned\|dashboardPinned\|previewOnly` | refused `unavailable` | **unsupported**: QML preview/pin flags with no Bead view (the OSD is event driven, B0051) |
| `brightness step up\|down` | daemon `brightness.adjust` on the focused output | done (B0026) |
| `brightness set X` | daemon `brightness.set {output, percent}`; X is the QML fraction 0..1, scaled and clamped; replies `"ok"` like QML | **new** |
| `keyboardLight step up\|down` | daemon `keyboard_light.adjust {step}` (N0036: wraps at max, stops at 0) | **new** |
| `pearls detach`, `pearls open launcher\|ai\|dashboard\|media` | panel layer | done (B0048) |
| `pearls open controls` | pearl of the control-centre view (`control`); the user's `pearlPanels` list still decides | **new** |
| `colorsApp open [IMAGE]` | Colors app (B0060) | done (mapping existed; guarded now) |
| `welcomeApp open PAGE` | Welcome app (B0059) | done (mapping existed; guarded now) |
| `welcomeApp refresh` | acknowledged, nothing to do | **no-op by design**: refresh re-reads the first-run flag after `nacre-welcome --enable/--disable`; Bead's Welcome keeps no such state (the once-only claim stays with Nacre) and must not be started by it |
| `menuPrompt open ID` | answers the waiting `nacre-menu pick\|input` at once with `{cancelled:true, error:"Selector prompts are not supported by this shell yet"}` through `nacre-helper menu-cli respond ID` | **unsupported, fails fast**: the selector prompt needs a launcher pick/input mode (own unit). Without this the tool would hang until its timeout. Helper path: `$NACRE_HELPER`, else `~/.local/share/nacre/shell/tools/nacre-helper` |
| `menuPrompt cancel ID` | acknowledged | no-op: nothing is ever open |

Not shell routes (Nacre-internal or native helpers) stay as listed in N0048.

## Bug found by the guard: `nacre settings PAGE` was rejected

The audited catalog had `nacre settings` with no parameters, so the one-argument form (`nacre-settings PAGE`, Welcome
tiles, `nacre settings sound`) failed `commands::validate` before `map_views` ever saw it. The catalog parameter list now
allows one optional `page` (`"optional": true`; `validate` accepts between the required and the total count).

## Drift guard

`route_coverage_tests.rs` loads `shell/shell-routes.json` and, for every route and every example, runs it through
`map_views` and asserts the handler class in its table (panel call, launcher request, app, acknowledged, runtime/daemon
call, or stated refusal). A route in the snapshot without a row fails ("add a handler or an exemption"), as does a row that
names no route. Further tests pin the explicit refusals, the new mappings and the selector-prompt response (fake helper).

## Apps wear the live palette (B0064 finding)

B0056 handed an app the live wallpaper palette only as a file at spawn, and only if the shell had already seen a palette.
Apps started early (or running while the wallpaper changed) kept the bundle palette (Bead yellow accent against the QML
grey-blue). Now an app process follows the daemon palette itself:

- General engine primitive `bead_wayland::PaletteFeed` (`current()`, `watch(cb)`) in `Options::palette_feed`; the runtime
  applies `current()` at start, sends `UiEvent::LivePalette` on each change, and stages `WallpaperPalette` into the open
  window and panel (`set_wallpaper_palette` for panels, `window_ui` for windows; a window opened later gets it too).
  Any framework user without a wallpaper consumer can use it; nothing Nacre-specific is in the crate.
- Adapter `wallpaper::Feed` (nacre-shell): the `wallpaper.active` topic's current pair palette (same colours the shell
  commits), stale/unavailable state keeps the last palette. `app::run` follows the topic for every app and sets the feed.
- The spawn-time palette file stays: it avoids a flash of the bundle palette before the first topic snapshot.

## Temporary duplicates

None. The spawn-time palette file is kept on purpose (first frame); drop it 14 days after the live feed is trusted on the
desktop if a measured first frame shows no flash without it.

## Checks and evidence

See the report for the full remote CI (`ci.sh`) and the parity harness run. Goldens: none changed (no view changed).
Added/removed product lines: see the report.
