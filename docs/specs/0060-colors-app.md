# B0060 - Colors app (wallpaper colour explorer)

Status: **Review** (Claude worker, branch `task/b0060-colors-20261023`). Owner 2026-10-23: no QML after the switch and every
feature looks and feels the same. Reference: Nacre `origin/main` `nacre/shell/modules/colors/*`, `services/NacreColorsApp.qml`,
`NacreColours.qml`, `colour-data.js`, `crates/nacre-helper/src/colors{,_launch}.rs`, `desktop/nacre-colors.desktop`.
Pin `393b4aa6` (unchanged). **The explorer needs four Nacre methods that are not published yet (asks below); until then the
app opens, shows the active wallpaper's palette and applies nothing.**

## What it is

`nacre-shell --app colors [IMAGE]` (B0052 app mode): one xdg window `nacre-colors` (1100x760, min 760x560), own daemon client
(`nacre-app-colors`, plain client, no provider), single instance, exits when the window closes. `nacre colorsApp open [IMAGE]`
(the call `nacre-colors` makes) opens it or deep-links the running one (`commands::map_views`, `app::APPS`). Nothing is resident
in the always-on shell.

| QML piece | Bead |
| --- | --- |
| `NacreColorsWindow` header (logo, title, Studio/Compare/Accessibility, dark/light preview) | `shell/colors/main.luau` `header`, `kit.TabStrip`, `kit.IconButton` |
| `NacreColorsView` rail (search, wallpaper list with thumbnails, Static/Animated) | `rail`/`entry`, `kit.TextInput`, `kit.Paragraph` |
| Studio: photo with click-to-pick, colour families, "Other shades", style chooser and text, desktop preview, accent targets, fine-tune (tint, brightness, strength, time shift, workspace chips, reset) | `studio`, `kit.ImagePick`, `kit.ColorSampleRow`, `kit.ChipRow`, `kit.PreviewCard`, `kit.ColorTarget`, `kit.Slider` |
| Compare (one preview per style), Accessibility (contrast, vision simulation, warnings, audit rows) | `compare`, `access` |
| Footer (status/error, Export, Apply / In use) | `footer` |
| `NacreThemePreview` | `kit.PreviewCard` (same geometry and sample texts, roles from the palette map) |
| `NacreColorSample` / `NacreAccentTarget` | `kit.ColorSample`, `kit.ColorTarget` |
| `NacreColorsApp` (request, debounce, helper `serve`, actions, `applied`) | `nacre-shell/src/colors.rs` (adapter) |
| `NacreColours` + `colour-data.js` (live palette to the shell) | already done by B0021/B0026 (`wallpaper.active`); not part of this app |
| `colorsApp open` IPC, `nacre-colors` launcher | `commands::map_views` route; the Nacre launcher is unchanged |

The adapter is the only Nacre-specific code (method names, store shape); layout, order and wording are Luau and i18n data.

## Daemon inventory: what the QML calls, and what Bead uses

QML runs `nacre-helper colors <serve|catalog|apply|export>` as child processes of the shell (`NacreColorsApp.qml`), not daemon methods.

| QML use | Bead | Status |
| --- | --- | --- |
| `settings.store.colors` (per-wallpaper saved choices `wallpapers[path]`, `lastApplied`) | topic, read only: default request for a wallpaper, "applied" test | **published** |
| current wallpaper + live palette (`NacreWallpapers.actualCurrent`, `NacrePresentation.active`) | `wallpaper.active` topic: starting wallpaper, default preview mode, "applied" test, the unavailable view | **published** |
| `colors catalog` (wallpaper list, thumbnails, current) | `colors.catalog` | **ask A1** |
| `colors serve` (preview of one request: palette, accessibility, comparisons, image colours) | `colors.preview` | **ask A2** |
| `colors apply` (writes `colors.json`, sets wallpaper/palette, streams presentation) | `colors.apply` | **ask A3** |
| `colors export` (writes palette files to a folder) | `colors.export` | **ask A4** |
| thumbnails and the preview poster (helper cache paths) | PNG files in two daemon cache directories | **ask A5** |
| Settings Colors page: publisher target status (`theme-targets`: current / tool-missing) and the targets' write | `settings.set colors appThemes.<key>` exists; status has no route | **ask A6** |
| "Open image" (`FileDialog`) and drop of a file onto the window | none | **ask A7** (optional) |
| header dark/light | changes the *preview* mode only (as QML), no daemon call | n/a |
| `appearance.set_mode`, `wallpaper.next` | not used by this app (they are control-center tiles, B0057) | n/a |

### The asks (exact shapes the Bead adapter parses; all Desktop class, v1, unknown fields ignored)

Method presence in the welcome registry is the capability gate: the explorer needs `colors.catalog` and `colors.preview`;
Apply needs `colors.apply`, Export needs `colors.export`. Each is checked at v1 with a connected daemon; an unadvertised
method never gets a call.

**A1 `colors.catalog` `{}`** (timeout 10 s) returns
`{current: "<abs path>", entries: [{path, name, dynamic: bool, thumbnail: "<abs png path or empty>"}], favorites: [...], history: [...], choices: {...}}`.
At most 512 entries are read. `current` is the active wallpaper path (QML `data.media.path`); `favorites/history/choices` are
the helper's existing fields (not used by the first Bead version).

**A2 `colors.preview` `{request}`** (timeout 60 s). `request` is exactly the QML request object:
`{image, mode: dark|light, personality: natural|pigment|harmony|pop|mist|vivid|pearl|tide, backgroundFromWallpaper, overrides: {primary?, secondary?, tertiary?: "rrggbb"}, brightness (-0.15..0.15), saturation (0..1.6), hour (0..23), autoMode, tideAutomatic, timeTint, workspaceColors, favoriteId?, accent?: "rrggbb"|null, pick?: {x, y} (0..1 of the image), compare: bool}`.
`compare` is true only on the Compare page so the daemon computes `comparisons` only then. Bead sends at most one preview at a time, replaces a queued request by a newer one and drops a reply that is not for the newest request. The reply is the helper's `preview_result` with the palette flattened:

```json
{"id": "<identity>", "request": {...normalised...}, "name": "Mountain lake", "image": "<abs path>",
 "thumbnail": "<abs png path of the preview poster>", "frames": 1, "favorite": false, "otherCoverage": 0.12,
 "imageColors": [{"hex": "3a6ea5", "coverage": 0.42}],
 "colours": {"primary": "rrggbb", "...": "every role of palette.colours"},
 "audit": {"textPasses": true, "minimumTextContrast": 7.12, "warnings": ["..."],
           "text": [{"foreground": "rrggbb", "background": "rrggbb", "ratio": 14.2}],
           "simulated": {"protan": {role: "rrggbb"}, "deutan": {...}, "tritan": {...}}},
 "comparisons": [{"personality": "natural", "colours": {role: "rrggbb"}}]}
```

`colours` and `simulated.*` may carry the full role map (Bead keeps 17 roles). `comparisons` is empty unless `compare`. An error
reply (`validation`, message at most 240 characters) is shown as the status line and blocks Apply/Export until the next change.

**A3 `colors.apply` `{id}`** (timeout 60 s) returns `{applied: true, name}`. Effects reach Bead as `wallpaper.active` (pair) and
`settings.store.colors.lastApplied == id`; the button reads "In use" only when `lastApplied == id`, the active path equals
`image` and the active `surface` equals the preview's `colours.surface` (the QML test).

**A4 `colors.export` `{id}`** (timeout 60 s) returns `{directory: "<abs path>", files: ["..."]}`; Bead shows "Exported to <directory>".

**A5 cache images.** PNG only (Bead's `image` crate has no JPEG), plain regular files directly inside
`$XDG_CACHE_HOME/nacre/shell/colors-thumbs/` (list thumbnails, at most 256 px and 256 KiB, referenced by `entries[].thumbnail`)
and `$XDG_CACHE_HOME/nacre/shell/colors-preview/` (the preview poster, at most 1280 px and 2 MiB, `thumbnail` of the preview
reply, kept at the image's aspect ratio). Same rules as `player-art`/`screenshot-thumbs` (absolute, no symlink, size and pixel
limits); anything else shows a placeholder.

**A6 theme targets.** `colors.targets {}` returning `{targets: [{id, label, status: "current"|"stale"|"tool-missing"}]}` (the
installed `theme-targets.json` merged with publisher state), or the same data as a `settings.store.theme-targets` topic. The
toggles themselves are `settings.set {store: "colors", key: ["appThemes", id]}`, which the daemon already validates; please confirm
that a write re-themes the target (QML: `ClassicState` publishes after the store write) or add the call that does.

**A7 (optional) file chooser.** A daemon method that opens the portal FileChooser and returns a path, or a window DnD primitive.
Without it Bead cannot offer "Open image"; the image can still be given as the argument (`nacre-colors IMAGE`) or by a wallpaper
row in the rail.

## Framework pieces (kit, general and reusable)

`kit.ColorSample`/`ColorSampleRow` (swatch over a caption), `kit.ColorTarget` (named colour slot with lock and arm),
`kit.PreviewCard` (a miniature desktop from a role-to-RGBA map: bar, window, terminal, notification; labels are arguments),
`kit.ImagePick` (image slot with an invisible hit grid; actions get cell centres as fractions), `kit.TabStrip` (text tabs with an
underline), `kit.ChipRow` (toggle chips, `per_row` to a line), `kit.Paragraph` (wrapped text by average advance, `max_lines`
with an ellipsis; the kit has no text measurement). Image slots named `@icon:colors-*` are filled by the adapter at the slot's own
rectangle (`attach_images`): `photo` fits the poster whole on a transparent canvas, `card` and `thumb-N` crop to fill. This is the
existing `@icon:` attach pattern (`player_art`); the shared decoder gained `decode`/`encode` helpers. ADR 0004's list has the entry.

## Behaviour and deviations from the QML

- The request loop is the QML's (`change`, `generation`, one in flight, stale replies dropped) without the 60 ms slider debounce:
  at most one preview is in flight and the newest request waits for it, which coalesces a drag the same way.
- No drag and drop (Bead has no drag source/target primitive). Click a colour sample to use it as the accent; to give a sample to
  Second or Third, click that target first (it shows a ring: armed) and then the sample. The lock button pins the shown colour or releases it.
- Pick on the photo is a 16x10 hit grid (cell centre), not a pixel; the adapter maps it through the fitted image rectangle and ignores the bars.
- Hover on a colour sample does not highlight its regions on the photo (no hover action primitive); the hover ring is a fill change.
- No "Open image" button and no file drop (ask A7). Favorites are not listed (the QML window does not list them either).
- Dark/light header buttons and Ctrl+Return/Ctrl+F shortcuts: the buttons work; the two shortcuts are not mapped. Ctrl+W/Q close the window as for Settings.
- The header logo is the Nacre colours logo with the current shell palette baked in (not live).
- Text breaks use the Paragraph approximation, so a line can break a word earlier or later than Qt does.

## Look-and-feel parity (owner rule 2026-10-23)

`spikes/parity-compare.py` (B0056) is not on main and no nested QML + Bead session was run in this unit (the explorer is
blocked on the unpublished methods, so the QML app's real data path cannot be matched by a fake daemon). Done instead: the
geometry, sizes and wording were taken from the QML source (header 64 px with hairline, 16 px margins, rail 210/180 px with 88 px
rows, canvas split 58/42 at 720 px, photo ratio .56 capped at 460 px, preview card 14 px radius with the QML's inner positions,
accent targets 54 px, footer 80 px with the status above Export and Apply) and spacing/radii/text sizes come from the shared
size scale (same values as `NacreTokens`). Goldens (eyeballed): `colors-studio-1`, `-1.5`, `-studio-tall-1`, `-fine-1`,
`-compare-1`, `-access-1`, `-narrow-1`, `-unavailable-1`.

Known gaps (to close in the nested comparison once A1-A5 are published and `parity-compare.py` is on main):

| Item | Gap |
| --- | --- |
| Fonts and text metrics | goldens use the test font; Paragraph wrapping is approximate |
| Tab strip | label widths are estimated (8 px per character), the QML measures them |
| Sample hover ring, region overlay on the photo | not implemented (see above) |
| Mini logo in the preview | three small rounded squares, not the QML `BrandLogo` drawing |
| Animation | no sheet/tab transitions are specified in the QML Colors window besides the 160 ms logo reveal; none implemented |
| Window chrome | server-side decorations requested like Settings; the QML draws its own header drag area |
| Scroll bar | the kit scroll has no visible track; QML shows `NacreScrollBar` |
| Real wallpaper aspect and colour | goldens use gradient fixtures |

## Settings integration (B0058 slice 1 landed)

`shell/settings/registry/bead.json` lists `wallpaper` and `colors` as editable: their store rows (rotation, motion, collection
folder, palette source, style, readability contrast) are ordinary registry rows. Rows the registry marks `adapter`/`backend`/
`editorOwned` (palette mode, schedule, the 13 app-theming targets) stay "not available here" help rows, and the custom sections
(`wallpaper-picker`, `colors-preview`, `theme-bundles`, `publisher-targets`) show the placeholder line. Not done: a button that
opens this app from the Colors/Wallpaper pages (the Settings process has no route to a sibling app; `app::HOST` is configured only
in the shell) and the wallpaper picker, theme bundles and publisher target status (asks A6; bundles need the software library
methods, not specified here). Test `settings_wallpaper_and_colors_pages_are_editable_registry_rows`.

## Memory and lifecycle

App process only. Open: the window, its renderer and buffers, the adapter (two topics, one decoded poster at most 1280 px and
at most 40 list thumbnails at 100x60, generated PNGs for the slot sizes, 64 at most), the call tickets. Close: everything goes
with the process. The always-on shell gained a route string and an `APPS` entry and holds no Colors state. No memory capture was
run here (heavy lock not held), so there is no number: the app is a separate process judged on its own, and the only
extra data beyond the Settings window is the decoded poster and the generated PNGs.

## Tests

`crates/nacre-shell/src/colors_tests.rs`: no daemon methods (active palette shown, nothing applicable, actions inert);
view state and hostile input; fake daemon over a real socket (catalog, first preview with the active mode, one request at a
time, newest wins, apply payload and status, "In use" from the published pair, export, a refused preview, choose from the list by
catalog index, no calls to a daemon without the methods); image slots sized to their own rectangles and letterboxed; cache
directory guards; Settings pages; 8 goldens. Full remote CI job on prod (see the report for the exact run).

## Temporary duplicates

None. The QML Colors app stays until the switch is trusted (Nacre rule).
