# B0059 - Welcome app (the first-run tour as an on-demand Bead app)

> B0128 retired the standalone `b0059-welcome.py` experiment and its B0052-only arm. Current Welcome lifecycle tests and packaging CI remain.

Status: **Review** (Claude worker, branch `task/b0059-welcome-20261023`). Owner 2026-10-23: no QML after the switch and every
feature looks and feels the same. Reference: Nacre `origin/main` `nacre/shell/modules/welcome/*`
(`NacreWelcomeHost/Window/View/Row/Demo`, `welcome-catalog.js`), `services/NacreWelcomeApp.qml`, `bin/nacre-welcome`,
`desktop/nacre-welcome.desktop`, `crates/nacre-helper/src/launchers.rs` (N0026 native `welcome` helper). Pin `393b4aa6` (unchanged).

## What it is

`nacre-shell --app welcome [PAGE]` (B0052 app mode): one xdg window `nacre-welcome` (1020x760, min 720x540), own daemon client
(`nacre-app-welcome`, plain client, no provider), single instance, exits when the window closes. `welcomeApp open PAGE` (the route
Nacre's launcher row and `nacre-welcome` already call; it is in the route catalog) opens it or deep-links the running one
(`commands::map_views`, `app::APPS`). `PAGE` is `home|colors|shortcuts|apps|ready|help`; anything else shows the first page.
Nothing is resident in the always-on shell.

| QML piece | Bead |
| --- | --- |
| `NacreWelcomeWindow` (`NacreAppHeader`, content, footer), `NacreWelcomeView` pages, `NacreWelcomeRow` | `shell/welcome/main.luau` on kit components (`kit.Button`; header, page builders, footer) |
| `welcome-catalog.js` (apps, help, optional-app guides), steps, section titles, links | `shell/welcome/catalog.luau`: data only, text as message ids |
| `NacreWelcomeApp` page/step/guide state, `advance/back/jump/setup` | `shell/welcome/behavior.luau`: two Luau actions (`go`, `guide`) and the stores |
| `NacreWelcomeApp.route/link/close`, availability, shortcut data, window size | `nacre-shell/src/welcome.rs` (adapter, the only Nacre-specific code) |
| `welcomeApp open` IPC, `nacre-welcome` | `commands::map_views` route to `app::route("welcome", "open", [page])` |
| `Ctrl+W`, Escape, Enter | `bead-wayland` Ctrl+W; Escape and Enter in the Welcome runtime arms (`reload.rs`) |

Tour pages (QML parity): Welcome (hero), Your colors, Getting around (shortcuts), Nacre apps (tiles, setup guides), You're ready,
Help (reachable from the last page). Footer: Back / Skip tour, five step dots (clickable), Next / Start using Nacre / Back to finish.

## Decisions

- **Framework rule.** Layout, order, wording, which apps and links appear: Luau and catalog data, no Rust. The adapter maps three
  closed vocabularies (route names, link names, settings page ids) to actions; text from the view never becomes a command, path or URL.
  Tiles name the store that says whether this desktop can open the app (`needs = "welcome.can.ai"`), so a third party's catalog can
  change the tiles without recompiling.
- **Shortcuts come from the keybind catalog, not a hardcoded list.** `menu.query {query:"", mode:"apps", route:"learn.shortcuts"}`
  (Nacre's menu route that reads the actual compositor bindings, the same rows B0026 opens in the launcher). Row name = action,
  row description = keys; engine order is kept; rows without keys are dropped; at most 200 rows of 160 characters. Asked once per
  daemon boot, only while the shortcuts page is shown (a restarted daemon asks again). States: loading, ready, empty, unavailable.
  QML's helper also grouped the rows (Apps and panels, Windows, ...); the menu route has no groups, so the list is flat (gap).
- **First-run trigger stays in Nacre.** `nacre-helper welcome` owns `welcome.json`, the once-only claim (`claim-login`,
  `acknowledge-login`, `release-login`) and the preferences; Bead only renders the window it is asked to open. After the switch the
  claim must open Welcome with `nacrectl ui welcomeApp open PAGE` instead of `quickshell ipc call welcomeApp open PAGE`
  (Nacre `launchers.rs open_shell_page`, N-E3). `finish-tour` (`tourCompleted`) is not written by Bead: `firstRunShown`, set by the
  claim's acknowledge, already ends first-run, so closing the tour needs no write (Nacre can drop `tourCompleted` or write it from the claim).
- **i18n: `bead.welcome.*`, not an extended namespace.** Bead's `tr()` fetches only the `bead` namespace. Extending it to
  `welcome.catalog.*` means a second fetch per revision, several id shapes (`welcome.welcome-view.text.*`, `ui.welcome-window.*`,
  `applications.welcome.title`, whose `ui` namespace is huge) and QML's mechanical ids. Only English and the developer pseudo
  language exist today, so no translation is lost. The 74 strings are `bead.welcome.<name>` entries of `shell/i18n/{en,context}.json`,
  the same authoring path as every other Bead surface (`tools/i18n-sync.py`, 3-part ids, a translator note each). Cost: 74 ids in the
  built-in table (about 12 KiB of text, about 10 KiB of heap per process). The Nacre ids `welcome.catalog.*`, `welcome.welcome-view.*`,
  `welcome.welcome-row.*`, `welcome.welcome-demo.*`, `ui.welcome-*` and `applications.welcome.title` become unused when QML is retired.
- **App tiles and routes (existing routes only).**

| Tile | Opens | Offered when |
| --- | --- | --- |
| Nacre Settings | `app::route("settings", "open")` | always |
| Nacre Colors | `app::route("colors", "open")` (B0060) | the build registers the app (`app::APPS`); the app shows its own unavailable view without Nacre's colours methods |
| Nacre AI | the AI sidebar app (`app::route("chat", "toggle")`, B0043) | the daemon publishes the `ai.chat` topic |
| Nacre Brain | `nacre-brain open` (Brain has no daemon route yet; QML ran it through `uwsm app`) | the daemon publishes `brain.recent` and `nacre-brain` is on `PATH` |

  An unavailable optional tile shows "Setup guide" and opens its guide card (QML behaviour); an unavailable required tile shows a
  dimmed "Not installed". A started app closes Welcome (QML behaviour). Help buttons open Settings at `system` (Maintenance) and
  `displays`; links (`xdg-open`) are a fixed table.
- **Window close from a view.** `bead-wayland` honours `CompanionVisibility(false)` in the window slot like the panel slot (6 lines):
  Skip tour, Start using Nacre and Escape end the window and, in app mode, the process. General: any window view can retire itself.
- **Text height.** The renderer wraps text inside its box but Luau cannot measure it. `shell/welcome/main.luau` estimates lines from
  the character count, the font size and the known content width (the adapter projects the window size; a resize projects again).
  Estimates err on the safe side: a block may leave a little extra space, never clip. A text-measurement primitive in `bead-ui`
  would replace `textH`/`inkH` (same remark as B0060 `kit.Paragraph`).
- **Page store name.** The tour position is `welcome.step`, not `welcome.page`: the shell persists every store whose id ends in
  `.page` in `~/.local/state/bead/panels.json` and would reopen the window on the last page.

## Look-and-feel parity (QML left, Bead right, same session)

Method: B0056's harness (`spikes/parity-compare.py`, not on main yet; a copy with six `welcome-*` states was used) on the desktop under
the heavy lock: headless labwc > nested niri 2880x1800 at scale 1.5 > QML shell (Nacre `393b4aa6`, `quickshell`) or Bead (this
branch, release, CPU renderer), the same standalone `nacre-helper daemon`, the owner's wallpaper/palette record copied read-only,
the same window opened with `welcomeApp open PAGE`. The window opens tiled at about half the output in both arms. Pixel numbers
come from the harness gallery. Pairs: `docs/specs/0059-evidence/` (left QML, right Bead, per page).

| Page | Pair | whole-screen rmse % | changed % |
| --- | --- | --- | --- |
| hero | [welcome-hero.jpg](0059-evidence/welcome-hero.jpg) | 8.43 | 5.71 |
| colors | [welcome-colors.jpg](0059-evidence/welcome-colors.jpg) | 10.32 | 9.27 |
| shortcuts | [welcome-shortcuts.jpg](0059-evidence/welcome-shortcuts.jpg) | 4.95 | 4.22 |
| apps | [welcome-apps.jpg](0059-evidence/welcome-apps.jpg) | 6.30 | 5.00 |
| ready | [welcome-ready.jpg](0059-evidence/welcome-ready.jpg) | 5.95 | 4.56 |
| help | [welcome-help.jpg](0059-evidence/welcome-help.jpg) | 8.58 | 6.08 |

The numbers cover the whole output (wallpaper, bar and the window) and are dominated by the palette, the logo and the bar
(bar rmse 7.7 % in every state, B0056's area), not by the window layout. In the nested daemon Brain shows Open in Bead because
`nacre-brain` is on this desktop's PATH; QML shows Setup guide (its helper also needs the Brain control script).

Matched: header (64 px, logo, title, section label right-aligned, 35 % rule), 24 px content margin, headings (25/23 pt), body and
muted text sizes, app tile geometry (radius 12, glyph at 18/20, title at 76/16, 76 px minimum, button 18 px from the right edge,
single-line tiles about 79 px like QML), shortcut rows (radius 10, 58 px minimum, 43 % title column, mono accent keys), help items
and rules, pill buttons (34 px, full radius, selected = primary), footer (64 px, rule at 30 %, dots 10 px / 10 px gap, centred),
hero proportions (logo 38 % of the canvas, name 6.5 %), "You're all set" page, Escape/Enter/Ctrl+W.

### Known gaps (each with its owner or the next step)

1. **Palette in the app process.** Bead's apps start with the bundle's static palette, so accent and surfaces differ from the live
   wallpaper palette (QML teal, Bead yellow in the evidence). B0056 hands the live palette to apps (`app::live_palette_file`); Welcome
   follows it with no change here.
2. **Logo colour.** `welcome/assets/nacre-logo.svg` is a copy of the bar's old fixed-palette logo (dark on a dark surface). B0056's
   palette-templated `*.themed.svg` replaces it: change the three `welcome/assets/nacre-logo.svg` paths in `main.luau` and delete the copy.
3. **App logos.** QML draws four `BrandLogo` illustrations (settings, colors, AI, brain; 44 px). Bead shows the Material glyph QML
   itself shows while its logo loads. Needs four themed SVGs from Nacre's `BrandLogo.qml`.
4. **Your colors page.** QML has a live demo: four wallpaper scenes (click applies the wallpaper), eight style chips, dark/light,
   a miniature desktop preview and the four palette swatches. Bead shows the four live palette swatches (the real palette roles
   of this window) and a button to Nacre Colors. The demo needs `demo-start` (snapshot/restore of the owner's appearance) and the
   wallpaper/appearance writes that B0060 asks Nacre to publish; with them this page can reuse `kit.ChipRow`, `kit.PreviewCard` and
   `kit.ImagePick`. The "You're all set" summary line (wallpaper, style, mode) is left out for the same reason.
5. **Shortcut groups.** Flat list, see Decisions. Needs a `group` field on `learn.shortcuts` rows (Nacre ask). The QML note about niri
   binding introspection (`bindingIntrospection === false`) has no counterpart in the menu route.
6. **Page transition.** QML slides the page 24 px and fades it (170/150 ms). Bead switches at once (no store-driven page transition
   in the kit yet); reduced motion would skip it anyway.
7. **Scrollbar** is not drawn (Bead's `Scroll` has none); wheel and keyboard scroll work as in Settings.
8. **Window size** is fixed at 1020x760 when the compositor gives no size; QML derived 64 % x 70 % of the output (capped at 1020x760,
   floored at 720x540). Identical on 1920x1200; a smaller output gets the compositor's tiling size.
9. **Hover and pressed states** were not captured (the harness drives no pointer); they use kit tokens (`hover_fill`), so B0056's token
   work applies. No focus ring until Tab or Enter is used (QML has none).
10. **Copy changes (intentional).** "Quickshell docs" became "niri guide"; "built with Quickshell" became "built with Bead" (and names
    niri); "Turn off Show at login below ... nacre-welcome opens it" lost its reference to a toggle the QML view never had and to a
    CLI Bead does not ship; the colors page intro describes what Bead shows. Reverse in `en.json` if the owner prefers the old text.

## Nacre asks (none block this unit)

- N-E3 (switch plumbing): `nacre-welcome --login` and the launcher row call `welcomeApp open PAGE` through `nacrectl ui` instead of
  `quickshell ipc`; window rules for app id `nacre-welcome` / title `Nacre Welcome` (QML used `org.quickshell`).
- `learn.shortcuts` rows with a `group` (gap 5). Colours/demo methods (gap 4, B0060 asks A1-A7).
- Delete the Nacre ids listed under i18n and `NacreWelcomeApp`/`modules/welcome` after the switch is trusted (clean-up pass b).

## Temporary duplicates

- `shell/welcome/assets/nacre-logo.svg` is a copy of `shell/ui/assets/nacre-logo.svg` (an asset must live under the entry's
  directory). Delete it when the themed logo of B0056 lands (it needs one themed file per view directory or a shared asset root):
  expiry 7 days after B0056 is on main.
- The QML Welcome stays until the switch is trusted (Nacre rule). No `.rollback` files.

## Checks

- Unit, fake-daemon and runtime tests (`welcome_tests.rs`, 8): shortcut rows over a real client against a fake daemon (`menu.query`
  shape, order, rows without keys dropped, asked once per boot), published topics enable AI and Brain, closed action tables,
  `PATH` rule, Next/Back/dots/Enter/Skip/Start/Escape through the runtime (the window asks to close), resize re-projection.
- 10 goldens, eyeballed (`welcome-{hero,colors,shortcuts,apps,apps-optional,guide,ready,help}-1`, `apps-min-1` at 720x540,
  `hero-1.5`): `shell/tests/goldens/welcome-*.png`.
- `commands::tests::welcome_open_is_an_app_route`, the i18n id test (74 ids, context notes), the Luau analyzer on the new files.
- Nested smoke (`spikes/b0059-welcome.py smoke`, desktop, CPU renderer, fake daemon): all steps true (no app before the first open, one window `nacre-welcome`, a second open deep-links the same process, client hello `nacre-app-welcome`, process gone after a window-initiated close and after a compositor close, shell survives); cold open IPC to first niri poll n=6 p50 105 ms, max 120 ms (budget 150 ms)
- Full CI job on dev-2: ubuntu 24.04 container (the `ci.yml` steps): fmt, clippy `-D warnings`, service boundary, the whole workspace test suite, renderer feature checks, facade and Luau analyzer on the new files, spikes unit tests, packaging tests, private services and tray lifecycle all pass; the wallpaper-video smoke is flaky (`crop_keeps_centre` failed in 2 of 3 runs, unrelated: no wallpaper code touched); `cargo deny` not run (no Cargo.toml or lock change).
- Product lines added (excluding tests and goldens): about 440 Rust (`welcome.rs` 319, hooks in `reload.rs`/`app.rs`/`commands.rs`/
  `transactions.rs` 112) and 305 Luau against QML's about 1,000 (view 518, app service 337, catalog 102, row, window, host).
