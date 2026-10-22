# B0026 — Keybind routes and settings/palette/i18n integration

Status: **Review — launcher-family and brightness keys implemented; six keys blocked on named contracts.**
Gap-list unit 9 (`docs/research/core-switch-gap-20261022.md`); roadmap Features 2/7/14.
Nacre reference: `origin/main` at `dedef897` (N0031 published).

## How a bind reaches Bead

Every shell-targeting bind (niri `bindings.kdl`, Hyprland `keybinding.lua`,
`shortcuts.lua`, all generated from `compositor-contract.json`) runs
`nacrectl ui TARGET METHOD ARGS...`. That is the daemon call `ui.dispatch
{target,method,args}`, forwarded to whichever process registered the `ui`
provider. Bead's provider (`ui_provider.rs`) validates against the audited
catalog and forwards to the shell's command socket, so on switch day the bind
files stay unchanged. Volume, mute, mic, lock (`loginctl`), screenshots,
workspace and window binds do not touch the shell and are unaffected.

## Route map (19 binds, 16 distinct commands)

"Done" is implemented before this unit; "B0026" is implemented here.

| Key (niri / Hyprland) | Command | Bead handling | Status |
| --- | --- | --- | --- |
| Super+Space | `nacre launcher ''` | Launcher panel; second press closes it (QML `openMode` toggle) | Done; toggle B0026 |
| Super+W | `nacre launcher '>wallpaper '` | Launcher on route `style.wallpapers` (QML opens its wallpaper mode) | B0026, see note 1 |
| Super+V | `nacre clipboard` | Launcher on `actions.clipboard` (QML's own non-native fallback) | B0026, note 1 |
| Super+. , Super+Ctrl+E | `nacre menu actions.emoji ''` | Launcher route `actions.emoji` | B0026, note 2 |
| Super+Ctrl+C | `nacre menu actions.capture ''` | Launcher route `actions.capture` | B0026, note 2 |
| Super+X , Super+Esc | `nacre menu power ''` | Launcher route `power` | B0026, note 2 |
| Super+K , Super+F1 | `nacre menu learn.shortcuts ''` | Launcher route `learn.shortcuts` | B0026, note 2 |
| Super+Tab | `nacre mode overview` | Launcher route `actions.windows` (window list) | Partial, note 3 |
| Super+Ctrl+Space | `nacre mode palette` | Launcher route `style` | Partial, note 3 |
| Super+O | `drawers toggle dashboard` | Dashboard panel toggle | Done |
| XF86MonBrightness Up/Down | `brightness step up\|down` | Daemon `brightness.adjust {output, delta:±5}` on the focused output | B0026, note 4 |
| Super+Z | `nacre hide` | Closes the open panel only | Partial, blocker A |
| XF86KbdBrightness Up/Down | `keyboardLight step up\|down` | `unavailable` | Blocked, blocker B |
| Super+A | `nacre left` | `unavailable` (no AI sidebar) | Blocked, blocker C |

Totals: 16 commands. Fully mapped 10 (launcher, wallpaper, clipboard, emoji,
capture, power, shortcuts, dashboard, brightness up, brightness down).
Mapped with a different presentation 2 (overview, palette). Partial 1 (hide).
Blocked 3 (keyboard light up/down, AI sidebar).

Notes. (1) QML's wallpaper and clipboard modes are native views; Bead shows the
engine's route in the launcher view. (2) The route opens and lists; activating a
non-application row is blocker D. (3) QML `overview` is a workspace grid and
`palette` a native picker; Bead has neither view, so the menu aliases Nacre's
own menu model defines (`overview` to `actions.windows`, `palette` to `style`)
are used. (4) Needs a daemon that advertises `brightness.adjust` v1; otherwise
the key replies `unavailable` and nothing happens.

### Route semantics

`menu` never toggles (QML `openMenu`); `launcher`, `mode` and `clipboard` close
an already-open identical request (QML `openMode`). Alias names are resolved in
`key_routes.rs` because the launcher fences replies by the route it requested and
the daemon echoes the resolved id. `legacy` mode and unknown modes are refused.
Routes are a bounded lowercase dotted id; the daemon owns validity and ranking.
Brightness sends one call at a time (the daemon rejects concurrent admission), is
sent without a retry, and uses the exact output name of the focused output.

## Blockers (exact)

- **A. hide/show frame.** Bead has no hidden state for bar and panel surfaces;
  `nacre hide` only dismisses an open panel. Needs a bead-wayland surface
  hide/show request (framework, generic) plus a bar toggle.
- **B. Keyboard backlight.** No published method. `brightness.*` is output-scoped
  and N0031 deliberately excludes keyboard light. Needs a Nacre unit publishing
  e.g. `keyboard_light.adjust` v1.
- **C. AI sidebar** (`left`, `leftDrawer`). Bead has no AI/Brain drawer; belongs
  to the Nacre AI/Brain surface units, not core-switch.
- **D. `ui.menu.activate`.** The daemon forwards every non-application launcher
  row (copy, copy-clip, power, capture, wallpaper, workflow, settings, ...) to the
  provider's `ui.menu.activate` v1. Bead registers only `ui.dispatch`, so those
  rows cannot be activated. This blocks the useful end of emoji, clipboard,
  capture, power and wallpaper. Next unit: handle the operations Bead can do
  safely (`copy`, `copy-clip`, `power` through `session.request`, `route`).
- **E. Brightness feedback.** `brightness.adjust` returns `{changed:true}` only;
  no readback is published, so the OSD cannot show the new level.

## Part 2: settings, palette, i18n

Palette. Bead's runtime palette is the wallpaper activation's palette (Orient
roles) committed with its image. Two findings, both fixed:

1. The palette reached only the bar session. A panel opened later, or open during
   a switch, kept the start-up file palette. `bead-wayland` now records the
   committed palette once (`set_wallpaper_palette`), stages it into an open panel
   and into every newly started panel before its first load.
2. `bead-ui` text-patching (`materialized::Cached::patch`) reused the cached
   scene, with its baked colors, whenever markup, viewport and bindings were
   unchanged, so a palette change alone repainted nothing. The cache now misses
   when the palette differs (about 2 KiB held per cached view).

Test: `live_palette_overlay_recolors_panel_views_without_reload_or_file_write`
drives the calendar and session views; it failed before fix 2 and now checks that
the overlay applies, survives a reload, and leaves the palette file untouched.
Not covered here: the compositor-level redraw of an open panel (needs a nested
session).

Catalog. `shell/command-catalog.json` (Nacre rev 68df8dd) lacked `nacre clipboard`,
so Super+V was refused by the provider; added, 121 routes. A scan of Nacre
`origin/main` found no other missing `registerIpc` method.

Settings. Bead reads no `settings.store.*` yet. Read-only consumers needed for
parity are `desktop` (motion, language, nativeClipboard), `dashboard` and
`dashboard-extras` (B0024 deliberately left them). No Bead-owned writer is added.
Nothing to wire without a consumer; each store lands with the view that needs it.

i18n. Not published to Bead. Nacre ships only `nacre/shell/i18n/en.json`
(`context.json` and `options.json` are metadata) read from the Nacre install by
QML and `nacre-i18n`; no protocol topic carries the catalog or the language, and
the daemon menu returns English names. Bead text is hard-coded English in Luau
and Rust (about 30 Luau `text=` sites and the B0023 messages), none with a
catalog ID. English is therefore complete today and nothing misleading is shown.
Wiring needs three pieces in order: (1) Nacre publishes the language and catalog
(a `desktop` store field plus a read route or a documented install path) and
adds IDs for the Bead strings; (2) a generic `bead-ui` `tr(id, fallback)` host
function with English fallback; (3) a live refresh on language change. No code
added now; a Rust translator without IDs or call sites would be dead.

## Temporary duplicates

None added by this unit.

## Acceptance

`cargo fmt`, clippy `-D warnings`, `cargo test --workspace --locked`, Luau
check, `cargo deny`, as in `.github/workflows/ci.yml`, on a remote host.
Nested smoke of the new routes is a follow-up (needs a daemon with the
`brightness.adjust` and menu routes); unit tests drive the same executor path.
