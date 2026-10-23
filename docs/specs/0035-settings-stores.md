# B0035 — Consume the owner's Nacre settings stores

Status: **Review — inventory complete; reduced motion, temperature unit and dashboard core tabs
implemented; the rest is listed with reasons below.**
Follows B0026 ("Bead reads no `settings.store.*` topics"). Nacre reference: `origin/main` at
`bc03d708`, `nacre/shell/**` (QML) and `crates/nacre-helper/src/daemon/*` (daemon).

## Why

On switch day the desktop must look and behave as the owner configured it in Nacre Settings.
The daemon publishes every Settings store as `settings.store.<id>` (payload `SettingsStore`:
`store, rev, exists, data, dirty, error`; raw file content, **no defaults** except
`desktop.interfaceGradients`). QML applies the defaults itself, so Bead must too.

## Design (ADR 0004: Nacre keys only in `nacre-shell`)

- `bead-core::animation::set_reduced_motion(bool)` (generic, MIT/Apache): every later
  `Animations::target` reaches its target at once, so no curve runs and nothing keeps
  redrawing. Both call sites (panel open/close, bar peek) go through it.
- `nacre-shell/src/prefs.rs` (GPL): follows `settings.store.desktop` and
  `settings.store.dashboard` (two permanent topics next to `menu`/`tray`/`i18n`; a few hundred
  bytes each), decodes the keys below with Nacre's defaults and type checks (a wrong-typed
  value falls back to the default, like QML), applies reduced motion, and republishes one
  effective-preferences Source (`Prefs`). Consumers subscribe to that Source and wake on a
  real change only; nothing is polled and nothing restarts.
- Read-only: Bead writes no setting. Without a daemon, an older daemon, or in previews and
  fixtures there is no preferences Source and every value is Nacre's default.
- Not used on purpose: the `settings.desktop` topic. It is published only after a UI calls
  `settings.desktop state`, which needs the compositor controller, so it is empty when Bead is
  the only UI. The raw `settings.store.desktop` is always there.

## Inventory (setting, where QML reads it, Bead status)

Status codes. **H** honoured before this unit (daemon, compositor/helper or an earlier
Bead unit). **I** implemented here. **A** Bead has no such surface yet (feature absent, not a
settings gap). **G** Bead-side gap, deferred (reason given). **B** Nacre contract blocker
(exact keys in the next section). **R** registered in the Settings registry but no QML or
daemon consumer exists, so there is nothing to honour.

### `desktop` (`nacre/desktop.json`)

| Key(s) | QML source | Bead |
| --- | --- | --- |
| `animations`, `reduceMotion` | `widgets/NacreTokens.qml` (`motionEnabled`, used by bar, drawers, pearls, panels) | **I** reduced motion, live |
| `weatherFahrenheit` | `services/NacreWeather.qml` (`format`) | **I** dashboard Today and Weather pages |
| `language` | `services/NacreI18n.qml` | **H** B0032 (daemon `i18n` topic carries the language) |
| `dnd` | `services/NacreNotifs.qml`, `NacreDashboardState.qml`, `DesktopActions.qml`, `NacreMenuContext.qml` | **H** the daemon notification service reads it and publishes `notifications.do_not_disturb`, which B0020/B0024 show and toggle |
| `weatherLocation` | `services/NacreWeather.qml` | **H** daemon `weather.rs` reads it |
| `nativeClipboard` | `modules/NacreShellIpc.qml` (`nacre clipboard`) | **H** both values reach the launcher clipboard route (B0026); Bead has no separate native clipboard view |
| `topBarEnabled`, `frameTopEnabled` | `drawers/NacreScreen.qml`, `NacrePanelInput.qml`, `topbar/NacreStatusHint.qml` | **I** B0036: `bead_wayland::ShellControl` bar-off state, live |
| `dashboardEnabled`, `controlCenterEnabled`, `aiSidebarEnabled` | `drawers/NacrePanelInput.qml`, `NacreFrameLips.qml`, `services/NacrePanelState.qml` | **I** B0036 for `dashboardEnabled` and `controlCenterEnabled` (panel gate plus dashboard button). `aiSidebarEnabled` is **A** (no AI sidebar, B0026 blocker C) |
| `workspaceWindowHints` | `topbar/NacreWorkspaceRow.qml` | **G** B0036: still open, Bead's capsule shows no window icons yet |
| `interfaceFont`, `monoFont`, `textSize` | `widgets/NacreTokens.qml` | **I** B0036: `bead_ui::text_style` alias table and size offset, live reload |
| `interfaceGradients` | `NacreTokens.qml`, `topbar/NacreWorkspacePearl.qml`, `dashboard/NacreDashboardHero.qml` | **A** Bead has no pearl or hero gradients; only the frame rim stroke (see `frameSheen`) |
| `frameSheen` | `drawers/NacreFrameRim.qml`, `NacrePanelHost.qml` | **I** B0036: no rim stroke when off; decoration variant rebuilds the tiles |
| `frameWidth`, `frameRounding`, `frameLeftEnabled`, `frameRightEnabled`, `frameBottomEnabled`, `leftEdge`..`bottomEdge` (legacy aliases) | `services/NacreHoverIntent.qml`, `drawers/*` | **I** B0047: live frame geometry, per-side toggles and reserved edges (`prefs.rs`, `placement::Frame`) |
| `clickEdgeMenus`, `lipHoverDelay`, `dragPanels`, `detachablePanels` | `drawers/NacreFrameLips.qml`, `NacrePanelInput.qml`, `services/NacrePearls.qml`, `NacrePanelDragRouting.qml` | **I** B0047 for `clickEdgeMenus` and `lipHoverDelay` (edge lips). **I** B0048 for `dragPanels` and `detachablePanels` (pearls) |
| `livePreviews`, `nativeOverview`, `nativePalette` | `modules/extras/Overview.qml` (`livePreviews` only) | **A** no overview or palette view (B0026 note 3); `nativeOverview` and `nativePalette` have no shell reader |
| `lockMedia`, `lockWeather`, `lockNotifications`, `lockNotificationContents` | `services/LockWidgets.qml` | **A** the lock screen is the external locker (B0023), not Bead |
| `blur`, `shadow`, `followMouse`, `naturalScroll`, `gapsIn`, `gapsOut`, `borderSize`, `rounding`, `keyRepeat*`, `mouse*`, `touchpad*`, `shakeCursor` (not read), `niriWorkspaceMode`, `workspacePrimaryOutput`, `monitorWorkspaceActions` | applied by `desktop-settings` / `nacre-helper` to the compositor config, not rendered by the shell | **H** independent of the UI shell |
| `lipSize`, `bottomPanel`, `modules`, `clockFormat`, `tray`, `notificationRetention`, `clipboardRetention`, `screenshotRetention`, `aiChatRetention` | none (registry only) | **R** see Nacre findings |

### Other stores

| Store / key | QML source | Bead |
| --- | --- | --- |
| `dashboard.tabs` | `services/NacreDashboardState.qml` (`coreTabs`), registry `NacreDashboardRegistry.core` | **I** core tab order and visibility, including the QML default order (today, media, weather, system) when a daemon is connected; unknown names dropped, legacy five-name list = unset, never empty. Previews and fixtures keep the fixed set so the goldens stay valid |
| `dashboard.lyrics`, `albumPalette`, `homeZone`, `homeCity`, `calendarFile` | daemon `dashboard_data.rs` / `dashboard_media.rs` | **H** the daemon applies them before publishing `dashboard.*` |
| `dashboard.focusMinutes` | `NacreDashboardState.qml`, `extras/NacreFocusExtra.qml` | **H** the daemon default; Bead's Focus page sends explicit minutes (B0027) |
| `dashboard-extras.enabled`, `order` | `NacreDashboardState.qml` | **H** B0027 (read and `settings.set` write) |
| `dashboard-extras.weatherPhotos` | `dashboard/NacreWeatherPage.qml`, `NacreWeatherSky.qml` | **A** Bead's Weather page has no photo hero |
| `control-center.tiles` | `services/NacreControlPreferences.qml`, `modules/controlcenter/*` | **A** Bead's right-edge panels are the fixed audio, network, Bluetooth and battery views (B0018), not the tile grid |
| `launcher` (`favorites`, `hidden`, usage), `launcher-sources` | `LauncherPreferences.qml`, `NacreMenuContext.qml` | **H** the daemon menu engine reads both files and filters and ranks before `menu.query` answers (`daemon/menu.rs`) |
| `launcher-layout` (`order`, `hidden`) | `NacreMenuContext.qml` (`Model.rail`) | **G** B0036: still open, Bead's launcher has no category rail |
| `launcher-menu`, `launcher-workflows`, `apps` | `NacreMenuContext.qml`, `NacreLauncherWorkflows.qml` | **H** custom entries, workflows and default apps are daemon/helper-owned and arrive as menu rows |
| `colors`, `wallpaper-picker` (`palette*`, `layout`, `kind`) | `NacreColorsApp.qml`, `NacreColours.qml`, `NacreWallpapers.qml` | **H** the daemon generates the palette and publishes `wallpaper.active`; Bead applies it (B0021, B0026) |
| `wallpaper-picker.motionMode`, `paused`, `pauseCovered` | `services/WallpaperPlayback.qml` | **A** Bead's wallpaper is static (B0021 non-goal: video) |
| `wallpaper-picker.rotationEnabled`, `rotationMinutes`, `rotationKind`, `rotationShuffle`, `rotationAnchorMs` | `services/NacreWallpapers.qml` (QML timer) | **B** nobody rotates wallpapers once QML is gone |
| `power` (`enabled`, `ac`, `battery`) | `services/NacrePowerPolicy.qml` (runs `powerprofilesctl set`) | **B** a shell-side duty with no daemon owner |
| `power.alerts`, `battery-alerts` | `services/NacreBatteryAlerts.qml` (low-battery notices) | **B** same |
| `power.idle.*`, `lid`, `button`, `saver.enabled` | helper (`desktop-settings`) | **H** compositor/logind side; not a shell duty |
| `profile.workspaceCount`, `workspaceNames`, `workspaceIcons` | `config/NacreBar.qml` into `topbar/NacreWorkspaceRow.qml` | **G** B0036: still open, see its table (icons need a capsule icon mode, count needs backend `fixedSlot`/`visiting`; names are Overview-only) |
| `profile.components` (AI, Brain) | `services/NacreProfile.qml` | **A** Bead has no AI or Brain surfaces |
| `cli`, `scheme`, `theme-targets`, `software-input`, `welcome` | Settings app and helper tools | **A** not read by always-on UI |

Counts (rows above, a row is one setting or a group read by the same code): honoured 12,
implemented here 3, Bead-side gaps 7, features absent in Bead 10, Nacre contract blockers 3,
registry-only 1 row (9 keys).

## Nacre contract blockers (exact keys)

1. **Wallpaper rotation.** `wallpaper-picker.rotationEnabled`, `rotationMinutes` (5..1440),
   `rotationKind`, `rotationShuffle`, `rotationAnchorMs`. Scheduling, pool selection and
   pause rules live in `NacreWallpapers.qml` (about lines 77-310). Needs a daemon-owned
   scheduler (publish "next at" and call the existing activation), not a second timer in Bead.
2. **Automatic power profile.** `power.enabled`, `power.ac`, `power.battery`:
   `NacrePowerPolicy.qml` runs `powerprofilesctl set` on AC/battery changes. No daemon method
   exists; propose the daemon applies it (it already probes `powerprofilesctl`).
3. **Battery alerts.** `battery-alerts` (state), `power.alerts`: `NacreBatteryAlerts.qml`
   raises the low-battery notices and keeps their severity. Propose daemon ownership.

## Nacre findings (not blockers)

- **Registry keys with no consumer.** `desktop.clockFormat`, `modules`, `tray`, `lipSize`,
  `bottomPanel`, `notificationRetention`, `clipboardRetention`, `screenshotRetention`,
  `aiChatRetention` appear in `settings/registry/settings.json` (the user can change them)
  but nothing in `nacre/shell` or `crates/` reads them (searched `origin/main`,
  `bc03d708`). Setting them changes nothing today; there is no 12/24-hour or date-format
  setting that the QML clock honours, so Bead's clock needs none.
- **`dashboard.tabs` default order** is today, media, weather, system in QML; Bead's fixture
  set was today, weather, media, system. Connected Bead now follows QML; the fixtures and
  goldens keep their order (changing them means regenerating about 30 PNGs).
- `settings.desktop` is empty until a UI calls `settings.desktop state` (see Design).

## Deferred Bead work (G rows), in order of value (B0036 closed items 1, 3 and `frameSheen`; the rest stays open, see [B0036](0036-settings-gaps.md))

1. Panel enable flags (`dashboardEnabled`, `topBarEnabled`, `controlCenterEnabled`) through
   the bar store plumbing in `bar_routes.rs` plus a refusal in `commands::map_views`.
2. `profile.workspaceNames/Icons/Count` in the workspace capsule.
3. Font family, mono family and text size: an alias table consulted by `bead-ui` layout.
4. `launcher-layout` once the launcher has a category rail.
5. `frameSheen` and `workspaceWindowHints`.

## Tests

- `bead-core`: `reduced_motion_reaches_every_target_at_once_and_keeps_nothing_active`.
- `prefs`: defaults and wrong types, motion rule, core-tab normalisation, live republish with
  no spurious wake on unrelated keys, wrong store id ignored, and a wire test through the real
  `nacre_source::Client` with a fake daemon (two stores published, one edited, motion and the
  effective Source follow).
- `dashboard_more_tests`: Fahrenheit conversion across the Weather page and live switch,
  Celsius without a preferences Source, wake on change, core tab order, hidden core tab,
  open page always listed.

## Temporary duplicates

None. Added product lines: about 190 (prefs 150, dashboard 30, bead-core 15); no removals.

## Memory

Two extra permanent topic mirrors (`settings.store.desktop`, `settings.store.dashboard`, a
few hundred bytes of JSON each) and one 100-byte effective-preferences snapshot. Estimated
under 10 KiB total anonymous heap; no matched capture was scheduled against the +0.5 MiB
gate.

## Acceptance

Remote CI-equivalent (fmt, clippy `-D warnings`, workspace tests, trace tests, feature
checks, facade, Luau analysis, spikes tests, cargo deny) on a build host.
