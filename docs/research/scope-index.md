# Static scope location index

Source: nacre-desktop/nacre `b547d06a02f8ff0dc0a6f76d366be086657e0bea`, audited 2026-10-13.

Generated with `python spikes/inventory.py REFERENCE docs/research/scope-inventory.json`.
Counts are declarations/member mentions, not active runtime instances. Dynamic
paths/commands/targets retain their source expressions; do not guess resolved values.

## Every import

| Import | Files | First location |
| --- | ---: | --- |
| `".." as Dashboard` | 1 | `nacre/shell/modules/dashboard/overview/NacreMediaCard.qml:6` |
| `"../../dashboard/media"` | 1 | `nacre/shell/modules/settings/pages/NacreAudioNode.qml:5` |
| `"../../dashboard/overview/overview.js" as Calendar` | 1 | `nacre/shell/modules/bar/popouts/NacreCalendarPopup.qml:5` |
| `"../../notifications"` | 1 | `nacre/shell/modules/settings/pages/NacreNotificationPage.qml:4` |
| `"../branding/LogoData.js" as LogoData` | 1 | `nacre/shell/widgets/BrandLogo.qml:3` |
| `"../colors"` | 1 | `nacre/shell/modules/welcome/NacreWelcomeDemo.qml:4` |
| `"../i18n/translation.js" as Translation` | 1 | `nacre/shell/services/NacreI18n.qml:5` |
| `"../modules/launcher/app-browser.js" as Legacy` | 1 | `nacre/shell/services/NacreMenuContext.qml:6` |
| `"../modules/launcher/menu-model.js" as Model` | 1 | `nacre/shell/services/NacreMenuContext.qml:5` |
| `"../modules/pearls/geometry.js" as Geometry` | 1 | `nacre/shell/services/NacrePearls.qml:6` |
| `"../modules/settings/settings-catalog.js" as Catalog` | 1 | `nacre/shell/services/NacreSettingsApp.qml:5` |
| `"../utils/scripts/view-state.js" as Views` | 1 | `nacre/shell/services/DisplayRecovery.qml:5` |
| `"../utils/scripts/wallpaper-rotation.js" as Rotation` | 1 | `nacre/shell/services/NacreWallpapers.qml:6` |
| `"app-search.js" as Search` | 2 | `nacre/shell/services/NacreApps.qml:3` |
| `"calculator.js" as Calculator` | 1 | `nacre/shell/modules/launcher/NacreAppBrowser.qml:8` |
| `"colour-data.js" as Colours` | 1 | `nacre/shell/services/NacrePresentation.qml:6` |
| `"colour-data.js" as Data` | 1 | `nacre/shell/services/NacreColours.qml:6` |
| `"compositor" as Backend` | 1 | `nacre/shell/services/NacreCompositor.qml:4` |
| `"compositor/session.js" as Session` | 1 | `nacre/shell/services/NacreCompositor.qml:5` |
| `"contour.js" as Contour` | 1 | `nacre/shell/modules/drawers/NacreChrome.qml:3` |
| `"geometry.js" as Geometry` | 1 | `nacre/shell/modules/pearls/NacrePearlWindow.qml:11` |
| `"layout.js" as Layout` | 1 | `nacre/shell/modules/drawers/NacrePanelHost.qml:2` |
| `"media" as Controls` | 1 | `nacre/shell/modules/dashboard/NacreMediaPage.qml:7` |
| `"media/media.js" as Model` | 2 | `nacre/shell/modules/dashboard/NacreMediaPage.qml:8` |
| `"menu-model.js" as Menu` | 1 | `nacre/shell/modules/launcher/NacreAppBrowser.qml:7` |
| `"overview" as Cards` | 2 | `nacre/shell/modules/dashboard/NacreDashboardSummary.qml:5` |
| `"overview.js" as Overview` | 3 | `nacre/shell/modules/dashboard/overview/NacreCalendarCard.qml:4` |
| `"pages"` | 1 | `nacre/shell/modules/settings/NacreSettingsView.qml:6` |
| `"registry.js" as Registry` | 1 | `nacre/shell/modules/drawers/NacreScreen.qml:2` |
| `"resource-data.js" as Data` | 1 | `nacre/shell/services/NacreSystemUsage.qml:5` |
| `"services"` | 1 | `nacre/shell/lock-preview.qml:2` |
| `"settings-catalog.js" as Catalog` | 1 | `nacre/shell/modules/settings/NacreSettingsView.qml:7` |
| `"welcome-catalog.js" as Catalog` | 1 | `nacre/shell/modules/welcome/NacreWelcomeView.qml:5` |
| `"widgets"` | 1 | `nacre/shell/lock-preview.qml:1` |
| `QtMultimedia` | 3 | `nacre/shell/modules/background/NacreDesktopVideo.qml:2` |
| `QtQuick` | 201 | `nacre/shell/config/Appearance.qml:2` |
| `QtQuick.Controls` | 9 | `nacre/shell/modules/colors/NacreColorsView.qml:3` |
| `QtQuick.Controls.Basic` | 17 | `nacre/shell/modules/bar/popouts/NacreQuickList.qml:2` |
| `QtQuick.Dialogs` | 1 | `nacre/shell/modules/colors/NacreColorsView.qml:2` |
| `QtQuick.Effects` | 1 | `nacre/shell/modules/launcher/NacreWallpaperHex.qml:3` |
| `QtQuick.Layouts` | 6 | `nacre/shell/modules/controlcenter/NacreControlCenter.qml:2` |
| `QtQuick.Shapes` | 9 | `nacre/shell/modules/dashboard/NacreChamberGauge.qml:2` |
| `QtQuick.Window` | 8 | `nacre/shell/modules/background/NacreWallpaperScene.qml:2` |
| `Quickshell` | 103 | `nacre/shell/lock-preview.qml:3` |
| `Quickshell.Bluetooth as Bluez` | 1 | `nacre/shell/services/NacreBluetooth.qml:3` |
| `Quickshell.Hyprland as Native` | 2 | `nacre/shell/services/compositor/HyprlandBackend.qml:6` |
| `Quickshell.Io` | 61 | `nacre/shell/lock-preview.qml:4` |
| `Quickshell.Services.Mpris` | 2 | `nacre/shell/modules/dashboard/NacreMediaPage.qml:4` |
| `Quickshell.Services.Notifications` | 1 | `nacre/shell/services/NacreNotifs.qml:6` |
| `Quickshell.Services.Pipewire` | 4 | `nacre/shell/modules/bar/popouts/NacreSoundPopup.qml:2` |
| `Quickshell.Services.UPower` | 7 | `nacre/shell/modules/bar/components/NacreStatusIcons.qml:3` |
| `Quickshell.Wayland` | 8 | `nacre/shell/modules/background/NacreBackground.qml:3` |
| `Quickshell.Widgets` | 1 | `nacre/shell/widgets/NacreClip.qml:2` |
| `qs.config` | 23 | `nacre/shell/modules/background/NacreWallpaperScene.qml:6` |
| `qs.modules` | 1 | `nacre/shell/shell.qml:4` |
| `qs.modules.background` | 1 | `nacre/shell/shell.qml:6` |
| `qs.modules.bar.components` | 1 | `nacre/shell/modules/topbar/NacreHeader.qml:7` |
| `qs.modules.bar.popouts as Details` | 1 | `nacre/shell/modules/controlcenter/NacreControlCenter.qml:8` |
| `qs.modules.bar.popouts as Popouts` | 1 | `nacre/shell/modules/drawers/NacrePanelHost.qml:7` |
| `qs.modules.colors` | 1 | `nacre/shell/shell.qml:9` |
| `qs.modules.controlcenter` | 1 | `nacre/shell/modules/osd/NacreOsdPanel.qml:1` |
| `qs.modules.controlcenter as Controls` | 1 | `nacre/shell/modules/pearls/NacrePearlWindow.qml:10` |
| `qs.modules.dashboard as Dashboard` | 2 | `nacre/shell/modules/drawers/NacrePanelHost.qml:8` |
| `qs.modules.drawers` | 1 | `nacre/shell/shell.qml:5` |
| `qs.modules.extras` | 1 | `nacre/shell/shell.qml:7` |
| `qs.modules.extras as Extras` | 3 | `nacre/shell/modules/drawers/NacrePanelHost.qml:9` |
| `qs.modules.launcher as Launcher` | 2 | `nacre/shell/modules/drawers/NacrePanelHost.qml:10` |
| `qs.modules.notifications` | 2 | `nacre/shell/modules/bar/popouts/NacreHistoryPopup.qml:4` |
| `qs.modules.notifications as Notifications` | 1 | `nacre/shell/modules/drawers/NacrePanelHost.qml:11` |
| `qs.modules.osd as Osd` | 1 | `nacre/shell/modules/drawers/NacrePanelHost.qml:12` |
| `qs.modules.pearls` | 1 | `nacre/shell/shell.qml:10` |
| `qs.modules.pearls as Pearl` | 1 | `nacre/shell/services/NacrePearls.qml:7` |
| `qs.modules.session as Session` | 1 | `nacre/shell/modules/drawers/NacrePanelHost.qml:13` |
| `qs.modules.settings` | 1 | `nacre/shell/shell.qml:8` |
| `qs.modules.topbar` | 1 | `nacre/shell/modules/drawers/NacreScreen.qml:8` |
| `qs.modules.welcome` | 1 | `nacre/shell/shell.qml:11` |
| `qs.services` | 117 | `nacre/shell/config/NacreBar.qml:3` |
| `qs.utils` | 8 | `nacre/shell/modules/bar/components/NacreActiveTitle.qml:5` |
| `qs.widgets` | 109 | `nacre/shell/config/NacreAppearance.qml:3` |

## Every object type spelling

Includes Qt/Quickshell, project types and inline components. Complete locations
and namespace member references are in scope-inventory.json.

| Type | Declarations | First location |
| --- | ---: | --- |
| `ActionButton` | 173 | `nacre/shell/modules/bar/popouts/NacreBluetoothPopup.qml:30` |
| `AnimatedImage` | 3 | `nacre/shell/modules/background/NacreWallpaperScene.qml:117` |
| `Box` | 12 | `nacre/shell/modules/drawers/NacrePanelMask.qml:6` |
| `BrandLogo` | 11 | `nacre/shell/modules/colors/NacreThemePreview.qml:36` |
| `Canvas` | 6 | `nacre/shell/modules/dashboard/NacreAlbumHalo.qml:5` |
| `Card` | 2 | `nacre/shell/modules/launcher/NacreWallpaperPicker.qml:250` |
| `Cards.NacreCalendarCard` | 1 | `nacre/shell/modules/dashboard/NacreDashboardSummary.qml:18` |
| `Cards.NacreHostCard` | 1 | `nacre/shell/modules/dashboard/NacreDashboardSummary.qml:34` |
| `Cards.NacreMediaCard` | 2 | `nacre/shell/modules/dashboard/NacreDashboardSummary.qml:79` |
| `ChatCard` | 3 | `nacre/shell/modules/extras/LeftDrawer.qml:391` |
| `ChatPane` | 1 | `nacre/shell/modules/extras/LeftDrawer.qml:136` |
| `CircleControl` | 2 | `nacre/shell/modules/notifications/NacreNotice.qml:146` |
| `ClippingRectangle` | 1 | `nacre/shell/widgets/NacreClip.qml:4` |
| `ColorAnimation` | 2 | `nacre/shell/widgets/NacreClip.qml:22` |
| `Column` | 81 | `nacre/shell/modules/bar/components/NacreStatusIcons.qml:24` |
| `Component` | 47 | `nacre/shell/modules/controlcenter/NacreControlCenter.qml:281` |
| `Connections` | 65 | `nacre/shell/modules/background/NacreDesktopVideo.qml:42` |
| `Controls.NacreAlbum` | 1 | `nacre/shell/modules/dashboard/NacreMediaPage.qml:145` |
| `Controls.NacreControlCenter` | 1 | `nacre/shell/modules/pearls/NacrePearlWindow.qml:322` |
| `Controls.NacreMediaButton` | 5 | `nacre/shell/modules/dashboard/NacreMediaPage.qml:193` |
| `Controls.NacreMediaSlider` | 2 | `nacre/shell/modules/dashboard/NacreMediaPage.qml:226` |
| `Dashboard.NacreAlbumHalo` | 1 | `nacre/shell/modules/dashboard/overview/NacreMediaCard.qml:157` |
| `Dashboard.NacreDashboardPanel` | 2 | `nacre/shell/modules/drawers/NacrePanelHost.qml:162` |
| `Dashboard.NacreMiniPlayer` | 1 | `nacre/shell/modules/pearls/NacrePearlWindow.qml:333` |
| `Details.NacreBatteryPopup` | 1 | `nacre/shell/modules/controlcenter/NacreControlCenter.qml:301` |
| `Details.NacreBluetoothPopup` | 1 | `nacre/shell/modules/controlcenter/NacreControlCenter.qml:289` |
| `Details.NacreNetworkPopup` | 1 | `nacre/shell/modules/controlcenter/NacreControlCenter.qml:283` |
| `Details.NacreSoundPopup` | 1 | `nacre/shell/modules/controlcenter/NacreControlCenter.qml:295` |
| `DragHandler` | 1 | `nacre/shell/widgets/NacrePanelDrag.qml:15` |
| `DropArea` | 5 | `nacre/shell/modules/colors/NacreAccentTarget.qml:56` |
| `Extras.Clipboard` | 1 | `nacre/shell/modules/launcher/NacreLauncherPanel.qml:123` |
| `Extras.Keybindings` | 1 | `nacre/shell/modules/launcher/NacreLauncherPanel.qml:130` |
| `Extras.LeftDrawer` | 2 | `nacre/shell/modules/drawers/NacrePanelHost.qml:85` |
| `Extras.Overview` | 1 | `nacre/shell/modules/launcher/NacreLauncherPanel.qml:116` |
| `Extras.Palette` | 1 | `nacre/shell/modules/launcher/NacreLauncherPanel.qml:102` |
| `FastScroll` | 30 | `nacre/shell/modules/bar/popouts/NacreQuickList.qml:18` |
| `FileDialog` | 1 | `nacre/shell/modules/colors/NacreColorsView.qml:67` |
| `FileView` | 29 | `nacre/shell/modules/dashboard/overview/NacreHostCard.qml:51` |
| `Flickable` | 17 | `nacre/shell/modules/bar/popouts/NacreQuickList.qml:5` |
| `FloatingWindow` | 5 | `nacre/shell/lock-preview.qml:12` |
| `Flow` | 48 | `nacre/shell/modules/colors/NacreColorsView.qml:213` |
| `FocusScope` | 2 | `nacre/shell/modules/controlcenter/NacreControlCenter.qml:11` |
| `FontMetrics` | 3 | `nacre/shell/modules/colors/NacreColorsView.qml:44` |
| `Gradient` | 12 | `nacre/shell/modules/dashboard/NacreDashboardHero.qml:23` |
| `GradientStop` | 47 | `nacre/shell/modules/dashboard/NacreCompactMetric.qml:43` |
| `Grid` | 4 | `nacre/shell/modules/bar/popouts/NacreCalendarPopup.qml:48` |
| `GridLayout` | 5 | `nacre/shell/modules/controlcenter/NacreControlCenter.qml:113` |
| `GridView` | 2 | `nacre/shell/modules/extras/Overview.qml:91` |
| `HoverHandler` | 9 | `nacre/shell/modules/dashboard/NacreTodayPage.qml:251` |
| `HyprlandShortcut` | 1 | `nacre/shell/services/compositor/HyprlandBackend.qml:142` |
| `HyprlandWindow` | 1 | `nacre/shell/services/compositor/HyprlandBackend.qml:249` |
| `Image` | 21 | `nacre/shell/lock-preview.qml:21` |
| `IpcHandler` | 42 | `nacre/shell/modules/NacreShellIpc.qml:117` |
| `Item` | 95 | `nacre/shell/lock-preview.qml:31` |
| `Launcher.NacreLauncherPanel` | 2 | `nacre/shell/modules/drawers/NacrePanelHost.qml:211` |
| `LazyLoader` | 3 | `nacre/shell/modules/colors/NacreColorsHost.qml:4` |
| `Level` | 4 | `nacre/shell/modules/controlcenter/NacreControlLevels.qml:10` |
| `LinearGradient` | 6 | `nacre/shell/modules/dashboard/NacreCompactMetric.qml:38` |
| `ListModel` | 1 | `nacre/shell/services/NacreChatSession.qml:171` |
| `ListView` | 10 | `nacre/shell/modules/colors/NacreColorsView.qml:114` |
| `Loader` | 14 | `nacre/shell/modules/background/NacreWallpaperScene.qml:127` |
| `MediaPlayer` | 3 | `nacre/shell/modules/background/NacreDesktopVideo.qml:49` |
| `MonthButton` | 4 | `nacre/shell/modules/bar/popouts/NacreCalendarPopup.qml:36` |
| `MouseArea` | 27 | `nacre/shell/lock-preview.qml:163` |
| `MultiEffect` | 1 | `nacre/shell/modules/launcher/NacreWallpaperHex.qml:55` |
| `NacreAccentTarget` | 1 | `nacre/shell/modules/colors/NacreColorsView.qml:368` |
| `NacreActiveTitle` | 1 | `nacre/shell/modules/topbar/NacreHeader.qml:175` |
| `NacreAdjustSlider` | 2 | `nacre/shell/modules/colors/NacreColorsView.qml:425` |
| `NacreAiPage` | 1 | `nacre/shell/modules/settings/NacreSettingsView.qml:288` |
| `NacreAppBrowser` | 1 | `nacre/shell/modules/launcher/NacreLauncherPanel.qml:79` |
| `NacreAppHeader` | 3 | `nacre/shell/modules/colors/NacreColorsWindow.qml:32` |
| `NacreAppearancePage` | 1 | `nacre/shell/modules/settings/NacreSettingsView.qml:248` |
| `NacreAudioNode` | 1 | `nacre/shell/modules/settings/pages/NacreSoundPage.qml:35` |
| `NacreBackground` | 1 | `nacre/shell/shell.qml:19` |
| `NacreBacklight` | 1 | `nacre/shell/services/NacreBrightness.qml:83` |
| `NacreBatteryAlertPolicy` | 1 | `nacre/shell/services/NacreBatteryAlerts.qml:100` |
| `NacreBluetoothPage` | 1 | `nacre/shell/modules/settings/NacreSettingsView.qml:268` |
| `NacreChamberGauge` | 2 | `nacre/shell/modules/dashboard/NacreFocusPage.qml:28` |
| `NacreChatSession` | 2 | `nacre/shell/services/NacreAiViews.qml:19` |
| `NacreChrome` | 1 | `nacre/shell/modules/drawers/NacreScreen.qml:78` |
| `NacreClip` | 5 | `nacre/shell/modules/dashboard/NacreDashboardHero.qml:4` |
| `NacreColorSample` | 1 | `nacre/shell/modules/colors/NacreColorsView.qml:276` |
| `NacreColorsHost` | 1 | `nacre/shell/shell.qml:25` |
| `NacreColorsView` | 1 | `nacre/shell/modules/colors/NacreColorsWindow.qml:74` |
| `NacreColorsWindow` | 1 | `nacre/shell/modules/colors/NacreColorsHost.qml:6` |
| `NacreCompactMetric` | 1 | `nacre/shell/modules/dashboard/NacreDashboardSummary.qml:70` |
| `NacreControlCenter` | 1 | `nacre/shell/modules/osd/NacreOsdPanel.qml:3` |
| `NacreControlLevels` | 1 | `nacre/shell/modules/controlcenter/NacreControlCenter.qml:109` |
| `NacreControlTile` | 6 | `nacre/shell/modules/controlcenter/NacreControlCenter.qml:118` |
| `NacreDashboardCustomize` | 1 | `nacre/shell/modules/dashboard/NacreDashboardPanel.qml:156` |
| `NacreDashboardHero` | 4 | `nacre/shell/modules/dashboard/NacreFocusPage.qml:25` |
| `NacreDashboardNavigation` | 1 | `nacre/shell/modules/dashboard/NacreDashboardPanel.qml:52` |
| `NacreDashboardSummary` | 1 | `nacre/shell/modules/dashboard/NacreTodayPage.qml:215` |
| `NacreDesktop` | 1 | `nacre/shell/shell.qml:20` |
| `NacreDesktopPage` | 1 | `nacre/shell/modules/settings/NacreSettingsView.qml:252` |
| `NacreDisplaysPage` | 1 | `nacre/shell/modules/settings/NacreSettingsView.qml:256` |
| `NacreDockFilm` | 1 | `nacre/shell/modules/pearls/NacreDockGuide.qml:51` |
| `NacreDockGuide` | 1 | `nacre/shell/shell.qml:21` |
| `NacreFocusPage` | 1 | `nacre/shell/modules/dashboard/NacreDashboardPanel.qml:150` |
| `NacreFrameLip` | 3 | `nacre/shell/modules/drawers/NacreFrameLips.qml:50` |
| `NacreFrameLips` | 1 | `nacre/shell/modules/drawers/NacreScreen.qml:118` |
| `NacreFrameRim` | 1 | `nacre/shell/modules/drawers/NacreScreen.qml:84` |
| `NacreHeader` | 1 | `nacre/shell/modules/drawers/NacreScreen.qml:107` |
| `NacreHeaderForwarder` | 1 | `nacre/shell/modules/topbar/NacreHeader.qml:288` |
| `NacreHelperTask` | 12 | `nacre/shell/services/LauncherPreferences.qml:63` |
| `NacreIcon` | 47 | `nacre/shell/lock-preview.qml:122` |
| `NacreIdleLoader` | 4 | `nacre/shell/modules/colors/NacreColorsWindow.qml:66` |
| `NacreImage` | 3 | `nacre/shell/modules/colors/NacreColorsView.qml:133` |
| `NacreInteraction` | 33 | `nacre/shell/modules/bar/components/NacrePowerButton.qml:30` |
| `NacreLevelNotice` | 1 | `nacre/shell/modules/osd/NacreOsdEvents.qml:79` |
| `NacreLightChannel` | 2 | `nacre/shell/services/NacreBacklight.qml:3` |
| `NacreLockPage` | 1 | `nacre/shell/modules/settings/NacreSettingsView.qml:280` |
| `NacreMaintenancePage` | 1 | `nacre/shell/modules/settings/NacreSettingsView.qml:292` |
| `NacreMediaButton` | 1 | `nacre/shell/modules/settings/pages/NacreAudioNode.qml:53` |
| `NacreMediaPage` | 1 | `nacre/shell/modules/dashboard/NacreDashboardPanel.qml:137` |
| `NacreMediaSlider` | 1 | `nacre/shell/modules/settings/pages/NacreAudioNode.qml:62` |
| `NacreMenuButton` | 18 | `nacre/shell/modules/launcher/NacreAppBrowser.qml:584` |
| `NacreMenuContext` | 2 | `nacre/shell/modules/pearls/NacrePearlWindow.qml:112` |
| `NacreMenuForm` | 1 | `nacre/shell/modules/launcher/NacreAppBrowser.qml:1088` |
| `NacreMenuIcon` | 3 | `nacre/shell/modules/launcher/NacreAppBrowser.qml:692` |
| `NacreNetworkPage` | 1 | `nacre/shell/modules/settings/NacreSettingsView.qml:264` |
| `NacreNotice` | 4 | `nacre/shell/modules/bar/popouts/NacreHistoryPopup.qml:62` |
| `NacreNotificationEntry` | 1 | `nacre/shell/services/NacreNotifs.qml:164` |
| `NacreNotificationPage` | 1 | `nacre/shell/modules/settings/NacreSettingsView.qml:272` |
| `NacreNumberField` | 4 | `nacre/shell/modules/settings/pages/NacreAppearancePage.qml:166` |
| `NacreOverviewCard` | 3 | `nacre/shell/modules/dashboard/overview/NacreCalendarCard.qml:6` |
| `NacrePaletteTask` | 2 | `nacre/shell/services/NacreWallpapers.qml:419` |
| `NacrePaletteTile` | 2 | `nacre/shell/modules/settings/pages/NacreAppearancePage.qml:296` |
| `NacrePanelDrag` | 4 | `nacre/shell/modules/drawers/NacrePanelHost.qml:104` |
| `NacrePanelHost` | 1 | `nacre/shell/modules/drawers/NacreScreen.qml:99` |
| `NacrePanelInput` | 1 | `nacre/shell/modules/drawers/NacreScreen.qml:91` |
| `NacrePanelMask` | 1 | `nacre/shell/modules/drawers/NacreScreen.qml:52` |
| `NacrePopupContent` | 1 | `nacre/shell/modules/bar/popouts/NacrePopupPanel.qml:54` |
| `NacrePowerButton` | 1 | `nacre/shell/modules/topbar/NacreHeader.qml:286` |
| `NacreQuickList` | 4 | `nacre/shell/modules/bar/popouts/NacreBluetoothPopup.qml:40` |
| `NacreQuickSlider` | 2 | `nacre/shell/modules/bar/popouts/NacreSoundPopup.qml:47` |
| `NacreRangeSlider` | 1 | `nacre/shell/modules/controlcenter/NacreControlLevels.qml:92` |
| `NacreReservedEdges` | 1 | `nacre/shell/modules/drawers/NacreScreen.qml:55` |
| `NacreScreen` | 1 | `nacre/shell/modules/drawers/NacreDesktop.qml:9` |
| `NacreScrollBar` | 11 | `nacre/shell/modules/bar/popouts/NacreQuickList.qml:17` |
| `NacreSearchPanel` | 1 | `nacre/shell/modules/launcher/NacreLauncherPanel.qml:109` |
| `NacreSessionControls` | 1 | `nacre/shell/modules/session/NacreSessionPanel.qml:35` |
| `NacreSettingToggle` | 15 | `nacre/shell/modules/settings/pages/NacreAiPage.qml:33` |
| `NacreSettingsHost` | 1 | `nacre/shell/shell.qml:24` |
| `NacreSettingsPage` | 12 | `nacre/shell/modules/settings/pages/NacreAiPage.qml:5` |
| `NacreSettingsSection` | 35 | `nacre/shell/modules/settings/pages/NacreAiPage.qml:13` |
| `NacreSettingsView` | 1 | `nacre/shell/modules/settings/NacreSettingsWindow.qml:61` |
| `NacreSettingsWindow` | 1 | `nacre/shell/modules/settings/NacreSettingsHost.qml:6` |
| `NacreShellIpc` | 1 | `nacre/shell/shell.qml:23` |
| `NacreShellShortcuts` | 2 | `nacre/shell/modules/Shortcuts.qml:3` |
| `NacreShortcut` | 11 | `nacre/shell/modules/NacreShellShortcuts.qml:65` |
| `NacreSoundPage` | 1 | `nacre/shell/modules/settings/NacreSettingsView.qml:260` |
| `NacreStatusHint` | 1 | `nacre/shell/modules/topbar/NacreHeader.qml:183` |
| `NacreStatusIcons` | 1 | `nacre/shell/modules/topbar/NacreHeader.qml:223` |
| `NacreSurface` | 83 | `nacre/shell/modules/bar/components/NacrePowerButton.qml:5` |
| `NacreSystemPage` | 1 | `nacre/shell/modules/dashboard/NacreDashboardPanel.qml:144` |
| `NacreTabStrip` | 1 | `nacre/shell/widgets/NacreAppHeader.qml:46` |
| `NacreText` | 304 | `nacre/shell/lock-preview.qml:36` |
| `NacreTextField` | 16 | `nacre/shell/modules/colors/NacreColorsView.qml:95` |
| `NacreThemePreview` | 4 | `nacre/shell/modules/colors/NacreColorsView.qml:350` |
| `NacreTimePage` | 1 | `nacre/shell/modules/settings/NacreSettingsView.qml:284` |
| `NacreTodayPage` | 1 | `nacre/shell/modules/dashboard/NacreDashboardPanel.qml:124` |
| `NacreWallpaperBackdrop` | 1 | `nacre/shell/modules/launcher/NacreWallpaperPicker.qml:110` |
| `NacreWallpaperHex` | 1 | `nacre/shell/modules/launcher/NacreWallpaperPicker.qml:322` |
| `NacreWallpaperMotion` | 2 | `nacre/shell/modules/launcher/NacreWallpaperHex.qml:35` |
| `NacreWallpaperPicker` | 1 | `nacre/shell/modules/launcher/NacreLauncherPanel.qml:93` |
| `NacreWallpaperScene` | 1 | `nacre/shell/modules/background/NacreBackground.qml:24` |
| `NacreWeatherPage` | 1 | `nacre/shell/modules/dashboard/NacreDashboardPanel.qml:131` |
| `NacreWeatherSky` | 1 | `nacre/shell/modules/dashboard/NacreWeatherPage.qml:39` |
| `NacreWelcomeDemo` | 1 | `nacre/shell/modules/welcome/NacreWelcomeView.qml:147` |
| `NacreWelcomeHost` | 1 | `nacre/shell/shell.qml:26` |
| `NacreWelcomeRow` | 1 | `nacre/shell/modules/welcome/NacreWelcomeView.qml:303` |
| `NacreWelcomeView` | 1 | `nacre/shell/modules/welcome/NacreWelcomeWindow.qml:53` |
| `NacreWelcomeWindow` | 1 | `nacre/shell/modules/welcome/NacreWelcomeHost.qml:6` |
| `NacreWindow` | 5 | `nacre/shell/modules/background/NacreBackground.qml:9` |
| `NacreWorkflowsPage` | 1 | `nacre/shell/modules/settings/NacreSettingsView.qml:276` |
| `NacreWorkspaceAppIcon` | 1 | `nacre/shell/modules/topbar/NacreWorkspaceRow.qml:225` |
| `NacreWorkspacePearl` | 2 | `nacre/shell/modules/topbar/NacreWorkspaceRow.qml:169` |
| `NacreWorkspaceRow` | 1 | `nacre/shell/modules/topbar/NacreHeader.qml:168` |
| `Native.GlobalShortcut` | 1 | `nacre/shell/services/compositor/HyprlandShortcut.qml:59` |
| `NiriShortcut` | 1 | `nacre/shell/services/compositor/NiriBackend.qml:103` |
| `NiriWindow` | 1 | `nacre/shell/services/compositor/NiriBackend.qml:99` |
| `NotificationServer` | 1 | `nacre/shell/services/NacreNotifs.qml:215` |
| `Notifications.NacreNotificationStack` | 1 | `nacre/shell/modules/drawers/NacrePanelHost.qml:204` |
| `NumberAnimation` | 44 | `nacre/shell/modules/background/NacreWallpaperScene.qml:185` |
| `Osd.NacreOsdEvents` | 1 | `nacre/shell/modules/drawers/NacrePanelHost.qml:245` |
| `Osd.NacreOsdPanel` | 1 | `nacre/shell/modules/drawers/NacrePanelHost.qml:118` |
| `PanelWindow` | 2 | `nacre/shell/modules/pearls/NacreDockGuide.qml:10` |
| `ParallelAnimation` | 1 | `nacre/shell/modules/dashboard/NacreDashboardPanel.qml:96` |
| `PathAngleArc` | 4 | `nacre/shell/modules/dashboard/NacreChamberGauge.qml:33` |
| `PathSvg` | 8 | `nacre/shell/modules/drawers/NacreChrome.qml:39` |
| `Pearl.NacrePearlWindow` | 1 | `nacre/shell/services/NacrePearls.qml:26` |
| `PersistentProperties` | 2 | `nacre/shell/modules/drawers/NacreScreen.qml:27` |
| `Popouts.NacrePopupPanel` | 1 | `nacre/shell/modules/drawers/NacrePanelHost.qml:195` |
| `Popup` | 1 | `nacre/shell/modules/launcher/NacreAppBrowser.qml:1147` |
| `Poster` | 3 | `nacre/shell/modules/background/NacreWallpaperScene.qml:108` |
| `Process` | 56 | `nacre/shell/lock-preview.qml:171` |
| `ProfileButton` | 3 | `nacre/shell/modules/bar/popouts/NacreBatteryPopup.qml:76` |
| `PropertyAnimation` | 2 | `nacre/shell/modules/pearls/NacreDockGuide.qml:57` |
| `PwObjectTracker` | 3 | `nacre/shell/modules/bar/popouts/NacreSoundPopup.qml:18` |
| `QtObject` | 37 | `nacre/shell/config/Appearance.qml:4` |
| `Rectangle` | 72 | `nacre/shell/lock-preview.qml:27` |
| `Region` | 7 | `nacre/shell/modules/background/NacreBackground.qml:23` |
| `Repeater` | 91 | `nacre/shell/lock-preview.qml:133` |
| `Row` | 73 | `nacre/shell/lock-preview.qml:118` |
| `RowLayout` | 8 | `nacre/shell/modules/controlcenter/NacreControlCenter.qml:95` |
| `Scope` | 7 | `nacre/shell/modules/NacreShellIpc.qml:6` |
| `ScreencopyView` | 1 | `nacre/shell/modules/extras/Overview.qml:113` |
| `ScrollBar` | 7 | `nacre/shell/modules/extras/ChatPane.qml:98` |
| `ScrollView` | 2 | `nacre/shell/modules/extras/ChatPane.qml:290` |
| `SearchSurface` | 4 | `nacre/shell/modules/extras/Clipboard.qml:8` |
| `Session.NacreSessionPanel` | 1 | `nacre/shell/modules/drawers/NacrePanelHost.qml:154` |
| `Shape` | 11 | `nacre/shell/modules/dashboard/NacreChamberGauge.qml:23` |
| `ShapePath` | 12 | `nacre/shell/modules/dashboard/NacreChamberGauge.qml:28` |
| `ShellRoot` | 2 | `nacre/shell/lock-preview.qml:8` |
| `Shortcut` | 7 | `nacre/shell/modules/colors/NacreColorsWindow.qml:76` |
| `Singleton` | 58 | `nacre/shell/services/AppLaunch.qml:4` |
| `Slider` | 4 | `nacre/shell/modules/bar/popouts/NacreQuickSlider.qml:6` |
| `SpinBox` | 1 | `nacre/shell/modules/settings/pages/NacreNumberField.qml:5` |
| `SplitParser` | 23 | `nacre/shell/lock-preview.qml:175` |
| `StatusTarget` | 5 | `nacre/shell/modules/bar/components/NacreStatusIcons.qml:27` |
| `StdioCollector` | 23 | `nacre/shell/modules/pearls/NacrePearlWindow.qml:208` |
| `SystemClock` | 1 | `nacre/shell/services/NacreTime.qml:18` |
| `Text` | 15 | `nacre/shell/lock-preview.qml:56` |
| `TextArea` | 2 | `nacre/shell/modules/extras/ChatPane.qml:379` |
| `TextEdit` | 1 | `nacre/shell/modules/extras/ChatPane.qml:223` |
| `TextField` | 1 | `nacre/shell/widgets/NacreTextField.qml:4` |
| `TextInput` | 1 | `nacre/shell/modules/settings/pages/NacreNumberField.qml:23` |
| `Timer` | 98 | `nacre/shell/lock-preview.qml:184` |
| `ToolTip` | 2 | `nacre/shell/modules/dashboard/NacreTodayPage.qml:276` |
| `Transition` | 1 | `nacre/shell/modules/notifications/NacreNotificationStack.qml:57` |
| `Translate` | 1 | `nacre/shell/modules/osd/NacreLevelNotice.qml:50` |
| `Transport` | 3 | `nacre/shell/modules/dashboard/overview/NacreMediaCard.qml:203` |
| `Variants` | 4 | `nacre/shell/modules/background/NacreBackground.qml:7` |
| `VideoOutput` | 3 | `nacre/shell/modules/background/NacreDesktopVideo.qml:35` |
| `WallpaperAction` | 2 | `nacre/shell/modules/dashboard/NacreTodayPage.qml:161` |
| `WallpaperControl` | 3 | `nacre/shell/modules/dashboard/NacreTodayPage.qml:186` |
| `WheelHandler` | 3 | `nacre/shell/modules/launcher/NacreWallpaperPicker.qml:264` |

## Every IPC handler and method

- `"leftDrawer"` — `nacre/shell/modules/NacreShellIpc.qml:117`: `section(name: string): void`; `state(): string`
- `"nacre"` — `nacre/shell/modules/NacreShellIpc.qml:133`: `state(): string`; `restoreViews(data: string): void`; `preview(name: string): void`; `previewDashboard(index: int): void`; `controls(section: string): void`; `previewFeedback(channel: string): void`; `previewSliders(): void`; `previewLeft(): void`; `mode(name: string): void`; `menu(route: string, query: string): void`; `settings(): void`; `left(): void`; `hide(): void`; `launcher(query: string): void`; `session(): void`; `popout(name: string, center: real): void`; `close(): void`; `popupState(): string`; `galleryStep(delta: int): void`; `galleryState(): string`; `workspace(id: int): void`; `tab(index: int): void`
- `"drawers"` — `nacre/shell/modules/NacreShellShortcuts.qml:86`: `toggle(drawer: string): void`; `list(): string`
- `"wallpaperMotion-" + root.screenName` — `nacre/shell/modules/background/NacreDesktopVideo.qml:61`: `state(): string`
- `"wallpaperFrame-" + root.screenName` — `nacre/shell/modules/background/NacreWallpaperScene.qml:191`: `state(): string`
- `"barStatus-" + root.screenName` — `nacre/shell/modules/bar/components/NacreStatusIcons.qml:69`: `state(): string`
- `"leftEdge"` — `nacre/shell/modules/drawers/NacreDesktop.qml:15`: `state(): string`
- `"frame"` — `nacre/shell/modules/drawers/NacreDesktop.qml:28`: `state(): string`
- `root.detached ? "leftDrawerWindow" + root.visibilities.windowKey : "leftDrawer-" + root.screen.name` — `nacre/shell/modules/extras/LeftDrawer.qml:457`: `section(name: string): void`; `state(): string`
- `"overview"` — `nacre/shell/modules/extras/Overview.qml:223`: `state(): string`; `move(address: string, workspace: int): void`
- `root.detached ? "featureSearchWindow" + root.visibilities.windowKey : "featureSearch"` — `nacre/shell/modules/extras/SearchSurface.qml:26`: `query(text: string): void`; `state(): string`
- `root.detached ? "appBrowserWindow" + root.visibilities.windowKey : "appBrowser"` — `nacre/shell/modules/launcher/NacreAppBrowser.qml:1276`: `selectCategory(id: string): void`; `openRoute(id: string): void`; `state(): string`
- `"header-" + root.screen.name` — `nacre/shell/modules/topbar/NacreHeader.qml:293`: `state(): string`
- `"chatTitle"` — `nacre/shell/services/ChatWindowTitle.qml:12`: `state(): string`
- `"desktopActions"` — `nacre/shell/services/DesktopActions.qml:249`: `run(action: string, value: string): void`
- `"extras"` — `nacre/shell/services/DesktopExtras.qml:97`: `counts(): string`; `refresh(kind: string): void`; `search(query: string): void`
- `"desktopSettings"` — `nacre/shell/services/DesktopSettings.qml:120`: `state(): string`; `set(key: string, value: string): void`
- `"displayRecovery"` — `nacre/shell/services/DisplayRecovery.qml:143`: `refresh(name: string): bool`; `state(): string`
- `"lockWidgets"` — `nacre/shell/services/LockWidgets.qml:93`: `state(): string`
- `"batteryAlerts"` — `nacre/shell/services/NacreBatteryAlerts.qml:154`: `state(): string`
- `"brightness"` — `nacre/shell/services/NacreBrightness.qml:147`: `step(direction: string): void`; `set(value: real): string`; `state(): string`
- `"colorsApp"` — `nacre/shell/services/NacreColorsApp.qml:367`: `open(image: string): void`; `preview(value: string): void`; `close(): void`; `state(): string`
- `"paletteState"` — `nacre/shell/services/NacreColours.qml:87`: `state(): string`
- `"dashboardState"` — `nacre/shell/services/NacreDashboardState.qml:214`: `state(): string`
- `"keyboardLight"` — `nacre/shell/services/NacreKeyboardLight.qml:45`: `step(direction: string): void`; `set(value: real): void`; `state(): string`
- `"menuPrompt"` — `nacre/shell/services/NacreMenuPrompt.qml:118`: `open(ident: string): void`; `cancel(ident: string): void`; `state(): string`
- `"networkStatus"` — `nacre/shell/services/NacreNetwork.qml:129`: `state(): string`; `refresh(): void`
- `"notifs"` — `nacre/shell/services/NacreNotifs.qml:228`: `counts(): string`; `clear(): void`; `dismissKey(key: string): void`; `clearHistory(): void`
- `"pearls"` — `nacre/shell/services/NacrePearls.qml:511`: `open(kind: string): void`; `detach(): void`; `dock(key: string): void`; `close(key: string): void`; `gesture(): string`; `state(): string`
- `"mpris"` — `nacre/shell/services/NacrePlayers.qml:37`: `getActive(prop: string): string`; `list(): string`; `play(): void`; `pause(): void`; `playPause(): void`; `previous(): void`; `next(): void`; `stop(): void`
- `"presentationState"` — `nacre/shell/services/NacrePresentation.qml:106`: `state(): string`
- `"settingsApp"` — `nacre/shell/services/NacreSettingsApp.qml:98`: `open(page: string): void`; `close(): void`; `state(): string`
- `"settingsView"` — `nacre/shell/services/NacreSettingsApp.qml:119`: `open(page: string): void`; `state(): string`
- `"resources"` — `nacre/shell/services/NacreSystemUsage.qml:158`: `state(): string`
- `"clock"` — `nacre/shell/services/NacreTime.qml:23`: `state(): string`
- `"wallpaper"` — `nacre/shell/services/NacreWallpapers.qml:538`: `get(): string`; `set(path: string): void`; `configure(value: string): void`; `next(): void`; `previous(): void`; `toggleMotion(): void`; `list(): string`; `state(): string`
- `"weatherStatus"` — `nacre/shell/services/NacreWeather.qml:123`: `state(): string`
- `"welcomeApp"` — `nacre/shell/services/NacreWelcomeApp.qml:284`: `open(page: string): void`; `close(): void`; `refresh(): void`; `setStartup(value: bool): void`; `route(name: string): void`; `selectDemo(path: string): void`; `restoreDemo(): void`; `themeDemo(mode: string, personality: string): void`; `state(): string`
- `"sidebarChat"` — `nacre/shell/services/SidebarChat.qml:6`: `state(): string`; `send(text: string): void`; `stop(): void`
- `"wallpaperPlayback"` — `nacre/shell/services/WallpaperPlayback.qml:19`: `state(): string`
- `"compositorState"` — `nacre/shell/services/compositor/HyprlandBackend.qml:349`: `state(): string`
- `"nacreCompositor"` — `nacre/shell/services/compositor/NiriBackend.qml:125`: `shortcut(name: string): bool`

## Every Process declaration

- `nacre/shell/lock-preview.qml:171` — `{"id": ["reader"], "running": ["true"], "command": ["[\"/usr/bin/cat\", Quickshell.env(\"HOME\") + \"/.cache/nacre/lock-ready/dashboard.json\"]"]}`
- `nacre/shell/modules/extras/Clipboard.qml:44` — `{"running": ["root.visibilities.launcher && root.visibilities.launcherMode === \"clipboard\""], "command": ["[\"inotifywait\", \"-q\", \"-m\", \"-e\", \"close_write,moved_to\", \"--format\", \"%f\", Quickshell.env(\"HOME\") + \"/.cache/cliphist\"]"]}`
- `nacre/shell/modules/pearls/NacrePearlWindow.qml:61` — `{"id": ["fitWorker"], "command": ["[\"python3\", (Quickshell.env(\"NACRE_PEARL_TOOLS\") || Quickshell.env(\"HOME\") + \"/.local/share/nacre/shell/tools\") + \"/pearl-window.py\", \"fit\", root.title, Quickshell.processId.toString(), \"0\", \"0\", Math.min(640, root.screen.width - 40).toString(), Math.min(root.screen.height - 100, Math.max(500, Math.min(820, root.width * .58 + 96))).toString()]"], "onExited": ["{"]}`
- `nacre/shell/modules/pearls/NacrePearlWindow.qml:191` — `{"id": ["focusWorker"], "command": ["[\"python3\", (Quickshell.env(\"NACRE_PEARL_TOOLS\") || Quickshell.env(\"HOME\") + \"/.local/share/nacre/shell/tools\") + \"/pearl-window.py\", \"focus\", root.title, Quickshell.processId.toString()]"], "onExited": ["{"]}`
- `nacre/shell/modules/pearls/NacrePearlWindow.qml:202` — `{"id": ["worker"], "command": ["[\"python3\", (Quickshell.env(\"NACRE_PEARL_TOOLS\") || Quickshell.env(\"HOME\") + \"/.local/share/nacre/shell/tools\") + \"/pearl-window.py\", action, root.title, Quickshell.processId.toString(), px.toString(), py.toString(), root.ready ? \"0\" : root.implicitWidth.toString(), root.ready ? \"0\" : root.implicitHeight.toString()]"], "onExited": ["(exitCode, exitStatus) => {"]}`
- `nacre/shell/services/ChatWindowTitle.qml:69` — `{"id": ["reader"], "command": ["[\"python3\", Quickshell.env(\"HOME\") + \"/.local/share/nacre/shell/tools/window-chat-title.py\", requestPid.toString()]"], "onExited": ["root.next()"]}`
- `nacre/shell/services/DesktopExtras.qml:58` — `{"id": ["worker"], "stdinEnabled": ["true"], "command": ["[\"python3\", Quickshell.env(\"HOME\") + \"/.local/share/nacre/shell/tools/desktop-extras.py\", action]"], "onExited": ["{"]}`
- `nacre/shell/services/DesktopSettings.qml:111` — `{"id": ["worker"], "onExited": ["code => root.finish(code)"]}`
- `nacre/shell/services/DeviceActions.qml:21` — `{"id": ["worker"], "onExited": ["{"]}`
- `nacre/shell/services/DisplayRecovery.qml:129` — `{"id": ["lockLayout"], "command": ["[\"python3\", Quickshell.env(\"HOME\") + \"/.local/share/nacre/shell/tools/lock-config.py\"]"]}`
- `nacre/shell/services/DisplayRecovery.qml:133` — `{"id": ["fallback"], "stdinEnabled": ["true"], "command": ["[\"python3\", Quickshell.env(\"HOME\") + \"/.local/share/nacre/shell/tools/display-recover.py\", \"-\"]"]}`
- `nacre/shell/services/LockWidgets.qml:81` — `{"id": ["writer"], "stdinEnabled": ["true"], "command": ["[\"python3\", Quickshell.env(\"HOME\") + \"/.local/share/nacre/shell/tools/lock-prepare.py\"]"], "onExited": ["{"]}`
- `nacre/shell/services/Maintenance.qml:32` — `{"id": ["worker"]}`
- `nacre/shell/services/NacreBatteryAlerts.qml:147` — `{"id": ["sender"], "onExited": ["(code, status) => root.finish(code === 0 && status === 0)"]}`
- `nacre/shell/services/NacreBrightness.qml:111` — `{"id": ["discovery"], "onExited": ["{"]}`
- `nacre/shell/services/NacreChatSession.qml:129` — `{"id": ["composerWorker"], "stdinEnabled": ["true"], "onExited": ["root.nextComposer()"]}`
- `nacre/shell/services/NacreChatSession.qml:337` — `{"id": ["wire"], "stdinEnabled": ["true"], "command": ["[\"python3\", (Quickshell.env(\"NACRE_PEARL_TOOLS\") || Quickshell.env(\"HOME\") + \"/.local/share/nacre/shell/tools\") + \"/sidebar-chat.py\", \"client\"].concat(root.channel ? [\"--channel\", root.channel] : [])"], "onExited": ["{"]}`
- `nacre/shell/services/NacreColorsApp.qml:244` — `{"id": ["minJob"], "onExited": ["code => {"]}`
- `nacre/shell/services/NacreColorsApp.qml:251` — `{"id": ["previewServer"], "command": ["[\"nice\", \"-n\", \"10\", root.python, root.tool, \"serve\"]"], "stdinEnabled": ["true"], "onExited": ["code => {"]}`
- `nacre/shell/services/NacreColorsApp.qml:271` — `{"id": ["catalogJob"], "command": ["[root.python, root.tool, \"catalog\"]"]}`
- `nacre/shell/services/NacreColorsApp.qml:289` — `{"id": ["actionJob"], "command": ["[root.python, root.tool, action]"], "stdinEnabled": ["true"], "onExited": ["code => {"]}`
- `nacre/shell/services/NacreControlTools.qml:52` — `{"id": ["worker"], "command": ["[\"python3\", root.tool, root.action, ...(root.value ? [root.value] : [])]"], "onExited": ["{"]}`
- `nacre/shell/services/NacreDashboardSystem.qml:79` — `{"id": ["reader"]}`
- `nacre/shell/services/NacreHelpers.qml:93` — `{"id": ["server"], "command": ["[Quickshell.env(\"HOME\") + \"/.local/share/nacre/palette-runtime/venv/bin/python\", Quickshell.env(\"HOME\") + \"/.local/share/nacre/shell/tools/helper-service.py\"]"], "stdinEnabled": ["true"], "running": ["true"], "onExited": ["{"]}`
- `nacre/shell/services/NacreLightChannel.qml:132` — `{"command": ["[]"], "onExited": ["(exitCode, exitStatus) => {"]}`
- `nacre/shell/services/NacreLightChannel.qml:146` — `{"id": ["ddcReader"]}`
- `nacre/shell/services/NacreMediaDetails.qml:78` — `{"id": ["visualizer"], "command": ["[\"python3\", root.tool, \"spectrum\"]"], "running": ["root.spectrumVisible && root.player?.isPlaying === true && NacreTokens.motionEnabled && !WallpaperPlayback.sleeping && !WallpaperPlayback.locked"]}`
- `nacre/shell/services/NacreMenuContext.qml:977` — `{"id": ["actionWorker"], "stdinEnabled": ["true"], "command": ["[\"python3\", root.tools + \"/menu-backend.py\", action, value]"], "onExited": ["root.refresh(true)"]}`
- `nacre/shell/services/NacreMenuContext.qml:1034` — `{"id": ["brainWorker"], "stdinEnabled": ["true"], "command": ["[\"python3\", root.tools + \"/desktop-extras.py\", \"brain\"]"], "onExited": ["if (query !== root.brainWanted)"]}`
- `nacre/shell/services/NacreMenuPrompt.qml:59` — `{"id": ["reader"], "onExited": ["followup.restart()"], "command": ["[\"python3\", root.cli, \"read-request\", ident]"]}`
- `nacre/shell/services/NacreMenuPrompt.qml:91` — `{"id": ["writer"], "onExited": ["followup.restart()"], "stdinEnabled": ["true"], "command": ["[\"python3\", root.cli, \"respond\", ident]"]}`
- `nacre/shell/services/NacreNetwork.qml:94` — `{"id": ["monitor"], "command": ["[\"nmcli\", \"--colors\", \"no\", \"monitor\"]"], "running": ["true"], "onExited": ["{"]}`
- `nacre/shell/services/NacrePaletteTask.qml:102` — `{"command": ["[\"python3\", root.tool, \"serve\"]"], "stdinEnabled": ["true"], "running": ["root.transport === null"], "onExited": ["(code, status) => {"]}`
- `nacre/shell/services/NacrePearls.qml:150` — `{"id": ["layoutReader"], "running": ["NacreProfile.ready"], "command": ["[\"python3\", (Quickshell.env(\"NACRE_PEARL_TOOLS\") || Quickshell.env(\"HOME\") + \"/.local/share/nacre/shell/tools\") + \"/pearl-state.py\", \"load\"]"]}`
- `nacre/shell/services/NacrePearls.qml:164` — `{"id": ["layoutWriter"], "stdinEnabled": ["true"], "command": ["[\"python3\", (Quickshell.env(\"NACRE_PEARL_TOOLS\") || Quickshell.env(\"HOME\") + \"/.local/share/nacre/shell/tools\") + \"/pearl-state.py\", \"save\"]"], "onExited": ["(exitCode, exitStatus) => {"]}`
- `nacre/shell/services/NacreSettingsApp.qml:79` — `{"id": ["minimizeJob"], "onExited": ["exitCode => {"]}`
- `nacre/shell/services/NacreSystemUsage.qml:144` — `{"id": ["slow"], "command": ["[\"python3\", Quickshell.env(\"HOME\") + \"/.local/share/nacre/shell/tools/resource-state.py\"]"], "onExited": ["(exitCode, exitStatus) => {"]}`
- `nacre/shell/services/NacreWallpapers.qml:369` — `{"id": ["catalog"], "command": ["[\"python3\", root.tool, \"catalog\"]"], "onExited": ["(code, status) => {"]}`
- `nacre/shell/services/NacreWallpapers.qml:391` — `{"id": ["demoRestore"], "command": ["[\"python3\", root.tool, \"demo-restore\", token]"], "onExited": ["code => {"]}`
- `nacre/shell/services/NacreWallpapers.qml:466` — `{"id": ["importer"], "command": ["[\"python3\", root.tool, action]"], "stdinEnabled": ["action === \"import\""], "onExited": ["(code, status) => {"]}`
- `nacre/shell/services/NacreWallpapers.qml:494` — `{"id": ["warmer"], "command": ["[\"nice\", \"-n\", \"19\", \"python3\", root.tool, \"warm\"]"], "onExited": ["{"]}`
- `nacre/shell/services/NacreWallpapers.qml:509` — `{"id": ["watcher"], "command": ["[\"python3\", Quickshell.env(\"HOME\") + \"/.local/share/nacre/shell/tools/wallpaper-watch.py\"]"], "running": ["true"], "onExited": ["{"]}`
- `nacre/shell/services/NacreWeather.qml:99` — `{"id": ["reader"], "command": ["[\"python3\", (Quickshell.env(\"NACRE_DASHBOARD_TOOLS\") || Quickshell.env(\"HOME\") + \"/.local/share/nacre/shell/tools\") + \"/weather.py\"]"], "onExited": ["(code, status) => {"]}`
- `nacre/shell/services/NacreWelcomeApp.qml:251` — `{"id": ["worker"], "onExited": ["exitCode => {"]}`
- `nacre/shell/services/TimezoneSettings.qml:28` — `{"id": ["reader"], "running": ["true"], "command": ["[\"python3\", root.helper, \"state\"]"]}`
- `nacre/shell/services/TimezoneSettings.qml:46` — `{"id": ["action"], "onExited": ["code => {"]}`
- `nacre/shell/services/Updates.qml:16` — `{"id": ["check"], "command": ["[\"python3\", Quickshell.env(\"HOME\") + \"/.local/share/nacre/shell/tools/updates.py\"]"], "running": ["true"]}`
- `nacre/shell/services/WallpaperPlayback.qml:31` — `{"running": ["true"], "command": ["[\"python3\", Quickshell.env(\"HOME\") + \"/.local/share/nacre/shell/tools/wallpaper-media.py\", \"session\"]"]}`
- `nacre/shell/services/WallpaperPlayback.qml:47` — `{"running": ["true"], "command": ["[\"dbus-monitor\", \"--system\", \"type='signal',interface='org.freedesktop.login1.Manager',member='PrepareForSleep'\", \"type='signal',interface='org.freedesktop.DBus.Properties',member='PropertiesChanged',arg0='org.freedesktop.login1.Session'\"]"]}`
- `nacre/shell/services/compositor/HyprlandBackend.qml:175` — `{"id": ["reloadConfig"], "command": ["[\"hyprctl\", \"reload\"]"]}`
- `nacre/shell/services/compositor/HyprlandBackend.qml:233` — `{"id": ["panelWriter"], "command": ["[\"hyprctl\", \"eval\", value]"], "onExited": ["root.writePanelRegions()"]}`
- `nacre/shell/services/compositor/HyprlandBackend.qml:321` — `{"id": ["cursor"], "command": ["[\"hyprctl\", \"cursorpos\", \"-j\"]"]}`
- `nacre/shell/services/compositor/HyprlandBackend.qml:334` — `{"id": ["focusReader"], "command": ["[\"hyprctl\", \"activewindow\", \"-j\"]"]}`
- `nacre/shell/services/compositor/NiriBackend.qml:131` — `{"id": ["stream"], "running": ["true"], "command": ["[\"python3\", root.toolsPath + \"/compositor.py\", \"events\"]"], "onExited": ["(exitCode, exitStatus) => {"]}`
- `nacre/shell/services/compositor/NiriBackend.qml:181` — `{"id": ["worker"], "onExited": ["(exitCode, exitStatus) => {"]}`
- `nacre/shell/utils/NacreIcons.qml:59` — `{"running": ["true"], "command": ["[\"python3\", \"-c\", \"import platform; print(platform.freedesktop_os_release().get('PRETTY_NAME', 'Linux'))\"]"]}`

## Every FileView declaration

- `nacre/shell/modules/dashboard/overview/NacreHostCard.qml:51` — `{"path": ["root.active ? \"/etc/os-release\" : \"\""]}`
- `nacre/shell/modules/dashboard/overview/NacreHostCard.qml:56` — `{"id": ["uptimeFile"], "path": ["root.active ? \"/proc/uptime\" : \"\""]}`
- `nacre/shell/modules/settings/pages/NacreAppearancePage.qml:17` — `{"path": ["(Quickshell.env(\"XDG_STATE_HOME\") || Quickshell.env(\"HOME\") + \"/.local/state\") + \"/nacre/scheme.json\""], "watchChanges": ["true"], "onFileChanged": ["reload()"]}`
- `nacre/shell/services/DesktopSettings.qml:91` — `{"path": ["Quickshell.env(\"HOME\") + \"/.config/nacre/desktop.json\""], "watchChanges": ["true"], "onFileChanged": ["reload()"]}`
- `nacre/shell/services/LauncherPreferences.qml:92` — `{"id": ["watcher"], "path": ["Quickshell.env(\"HOME\") + \"/.config/nacre/launcher.json\""], "watchChanges": ["true"], "onFileChanged": ["reload()"]}`
- `nacre/shell/services/NacreBatteryAlerts.qml:110` — `{"id": ["ledger"], "path": ["NacrePaths.state + \"/battery-alert-state.json\""], "watchChanges": ["false"]}`
- `nacre/shell/services/NacreColorsApp.qml:340` — `{"path": ["Quickshell.env(\"HOME\") + \"/.config/nacre/colors.json\""], "watchChanges": ["true"], "onFileChanged": ["reload()"]}`
- `nacre/shell/services/NacreColours.qml:79` — `{"path": ["NacrePaths.state + \"/scheme/current.txt\""], "watchChanges": ["true"], "onFileChanged": ["reload()"]}`
- `nacre/shell/services/NacreControlTools.qml:43` — `{"path": ["Quickshell.env(\"HOME\") + \"/.local/state/nacre/control-tools.json\""], "watchChanges": ["true"], "onFileChanged": ["{"]}`
- `nacre/shell/services/NacreDashboardState.qml:182` — `{"path": ["Quickshell.env(\"HOME\") + \"/.config/nacre/dashboard.json\""], "watchChanges": ["true"], "onFileChanged": ["root.refresh()"]}`
- `nacre/shell/services/NacreDashboardSystem.qml:72` — `{"id": ["networkFile"], "path": ["\"/proc/net/dev\""]}`
- `nacre/shell/services/NacreI18n.qml:79` — `{"path": ["Qt.resolvedUrl(\"../i18n/\" + (root.language === \"pseudo\" ? \"en\" : root.language) + \".json\")"]}`
- `nacre/shell/services/NacreI18n.qml:91` — `{"path": ["Qt.resolvedUrl(\"../i18n/options.json\")"]}`
- `nacre/shell/services/NacreI18n.qml:101` — `{"path": ["Qt.resolvedUrl(\"../i18n/en.json\")"]}`
- `nacre/shell/services/NacreLightChannel.qml:107` — `{"path": ["root.present && root.device && !root.isDdc ? \"/sys/class/\" + (root.descriptor.kind === \"backlight\" ? \"backlight\" : \"leds\") + \"/\" + root.device + \"/brightness\" : \"\""], "watchChanges": ["true"], "onFileChanged": ["root.readNative()"]}`
- `nacre/shell/services/NacreLightChannel.qml:118` — `{"path": ["root.present && root.device && !root.isDdc ? \"/sys/class/\" + (root.descriptor.kind === \"backlight\" ? \"backlight\" : \"leds\") + \"/\" + root.device + \"/max_brightness\" : \"\""]}`
- `nacre/shell/services/NacreMenuContext.qml:938` — `{"path": ["root.shared ? \"\" : root.tools + \"/launcher-menu.json\""], "watchChanges": ["true"], "onFileChanged": ["reload()"]}`
- `nacre/shell/services/NacreMenuContext.qml:949` — `{"path": ["root.shared ? \"\" : Quickshell.env(\"HOME\") + \"/.config/nacre/launcher-menu.json\""], "watchChanges": ["true"], "onFileChanged": ["reload()"]}`
- `nacre/shell/services/NacreNotifs.qml:168` — `{"path": ["Quickshell.env(\"HOME\") + \"/.local/share/nacre/shell/tools/notification-policy.json\""]}`
- `nacre/shell/services/NacrePresentation.qml:98` — `{"path": ["NacrePaths.state + \"/presentation.json\""], "watchChanges": ["true"], "onFileChanged": ["reload()"]}`
- `nacre/shell/services/NacreProfile.qml:17` — `{"id": ["file"], "path": ["Quickshell.env(\"HOME\") + \"/.config/nacre/profile.json\""], "watchChanges": ["true"], "onFileChanged": ["reload()"]}`
- `nacre/shell/services/NacreSystemUsage.qml:101` — `{"id": ["cpuFile"], "path": ["\"/proc/stat\""]}`
- `nacre/shell/services/NacreSystemUsage.qml:107` — `{"id": ["memoryFile"], "path": ["\"/proc/meminfo\""]}`
- `nacre/shell/services/NacreSystemUsage.qml:113` — `{"id": ["loadFile"], "path": ["\"/proc/loadavg\""]}`
- `nacre/shell/services/NacreSystemUsage.qml:119` — `{"path": ["\"/proc/sys/kernel/osrelease\""]}`
- `nacre/shell/services/NacreWallpapers.qml:345` — `{"path": ["root.currentNamePath"], "watchChanges": ["true"], "onFileChanged": ["reload()"]}`
- `nacre/shell/services/NacreWallpapers.qml:356` — `{"path": ["NacrePaths.state + \"/wallpaper/media.json\""], "watchChanges": ["true"], "onFileChanged": ["reload()"]}`
- `nacre/shell/services/NacreWeather.qml:88` — `{"path": ["Quickshell.env(\"HOME\") + \"/.cache/nacre/weather.json\""]}`
- `nacre/shell/services/Updates.qml:27` — `{"path": ["Quickshell.env(\"HOME\") + \"/.cache/nacre/updates.json\""], "watchChanges": ["true"], "onFileChanged": ["reload()"]}`

## Every helper task declaration

- `nacre/shell/services/LauncherPreferences.qml:63` — `["command: [\"python3\", Quickshell.env(\"HOME\") + \"/.local/share/nacre/shell/tools/launcher-preferences.py\", \"load\"]"]`
- `nacre/shell/services/LauncherPreferences.qml:78` — `[]`
- `nacre/shell/services/NacreDashboardState.qml:209` — `[]`
- `nacre/shell/services/NacreMediaDetails.qml:51` — `[]`
- `nacre/shell/services/NacreMenuContext.qml:859` — `["command: [\"python3\", root.tools + \"/menu-backend.py\", \"files\", query]"]`
- `nacre/shell/services/NacreMenuContext.qml:890` — `["command: [\"python3\", root.tools + \"/menu-backend.py\", \"convert\", query]"]`
- `nacre/shell/services/NacreMenuContext.qml:920` — `["command: [\"python3\", root.tools + \"/menu-backend.py\", \"ownership\", ident]"]`
- `nacre/shell/services/NacreMenuContext.qml:961` — `["command: [\"python3\", root.tools + \"/menu-backend.py\", \"state\", ...(force ? [\"--force\"] : [])]"]`
- `nacre/shell/services/NacreMenuContext.qml:997` — `["command: [\"python3\", root.tools + \"/menu-backend.py\", \"packages\", query]"]`
- `nacre/shell/services/NacreNetwork.qml:80` — `["command: [\"python3\", Quickshell.env(\"HOME\") + \"/.local/share/nacre/shell/tools/network-state.py\"]"]`
- `nacre/shell/services/NacreNotifs.qml:190` — `["command: [\"python3\", root.helper, \"load\"]"]`
- `nacre/shell/services/NacreNotifs.qml:201` — `["command: [\"python3\", root.helper, \"save\"]"]`
- `nacre/shell/services/NacreWallpapers.qml:419` — `["command: [\"python3\", root.tool, \"select\", requestPath, \"--stream-presentation\"]"]`
- `nacre/shell/services/NacreWallpapers.qml:444` — `["command: [\"python3\", root.tool, Object.keys(value).some(key => key.startsWith(\"palette\")) ? \"theme\" : \"preferences\", \"--stream-presentation\"]"]`

## Every shortcut declaration

- `nacre/shell/modules/NacreShellShortcuts.qml:65` — `{"name": ["\"dismissHoverEdges\""], "onPressed": ["root.dismissPassive()"]}`
- `nacre/shell/modules/NacreShellShortcuts.qml:70` — `{"name": ["\"session\""], "onPressed": ["NacrePanelState.toggleSession()"]}`
- `nacre/shell/modules/NacreShellShortcuts.qml:75` — `{"name": ["\"launcher\""], "onPressed": ["root.launcherPress()"], "onReleased": ["root.launcherRelease()"]}`
- `nacre/shell/modules/NacreShellShortcuts.qml:81` — `{"name": ["\"launcherInterrupt\""], "onPressed": ["root.launcherInterrupt()"]}`
- `nacre/shell/services/NacreBrightness.qml:137` — `{"name": ["\"brightnessUp\""], "onPressed": ["root.increaseBrightness()"]}`
- `nacre/shell/services/NacreBrightness.qml:142` — `{"name": ["\"brightnessDown\""], "onPressed": ["root.decreaseBrightness()"]}`
- `nacre/shell/services/NacreNotifs.qml:223` — `{"name": ["\"clearNotifs\""], "onPressed": ["root.hidePopups()"]}`
- `nacre/shell/services/NacrePlayers.qml:65` — `{"name": ["\"mediaToggle\""], "onPressed": ["root.control(\"playPause\")"]}`
- `nacre/shell/services/NacrePlayers.qml:70` — `{"name": ["\"mediaNext\""], "onPressed": ["root.control(\"next\")"]}`
- `nacre/shell/services/NacrePlayers.qml:75` — `{"name": ["\"mediaPrev\""], "onPressed": ["root.control(\"previous\")"]}`
- `nacre/shell/services/NacrePlayers.qml:80` — `{"name": ["\"mediaStop\""], "onPressed": ["root.control(\"stop\")"]}`
- `nacre/shell/services/compositor/HyprlandShortcut.qml:59` — `{"onPressed": ["if (!root.replacing && root.alive) {"], "onReleased": ["if (!root.replacing && root.alive) {"]}`

## CLI IPC call sites and wrapper invocations

- `nacre/shell-tools/compositor_backends/niri_config.py:120` — `'if [ -n "${NACRE_QS_CONFIG:-}" ]; then exec "${NACRE_QS_BIN:-$HOME/.local/share/nacre/shell/bin/qs}" -p "$NACRE_QS_CONFIG" ipc call nacreCompositor shortcut '`
- `nacre/shell-tools/compositor_backends/niri_config.py:122` — `+ '; else exec "${NACRE_QS_BIN:-$HOME/.local/share/nacre/shell/bin/qs}" -c nacre ipc call nacreCompositor shortcut '`
- `nacre/shell-tools/control.sh:10` — `exec "$HOME/.local/share/nacre/shell/bin/qs" -c nacre ipc call "$target" step "${2:-up}" ;;`
- `nacre/shell-tools/control.sh:14` — `close) exec "$HOME/.local/share/nacre/shell/bin/qs" -c nacre ipc call nacre close ;;`
- `nacre/shell-tools/control.sh:19` — `exec "$HOME/.local/share/nacre/shell/bin/qs" -c nacre ipc call nacre launcher ">wallpaper " ;;`
- `nacre/shell-tools/control.sh:22` — `detach) exec "$HOME/.local/share/nacre/shell/bin/qs" -c nacre ipc call pearls detach ;;`
- `nacre/shell-tools/control.sh:23` — `pearl) exec "$HOME/.local/share/nacre/shell/bin/qs" -c nacre ipc call pearls open "${2:-launcher}" ;;`
- `nacre/shell-tools/control.sh:24` — `controls) exec "$HOME/.local/share/nacre/shell/bin/qs" -c nacre ipc call nacre controls "${2:-home}" ;;`
- `nacre/shell-tools/control.sh:25` — `settings) exec "$HOME/.local/share/nacre/shell/bin/qs" -c nacre ipc call nacre settings ;;`
- `nacre/shell-tools/control.sh:26` — `left) exec "$HOME/.local/share/nacre/shell/bin/qs" -c nacre ipc call nacre left ;;`
- `nacre/shell-tools/control.sh:28` — `palette|overview) exec "$HOME/.local/share/nacre/shell/bin/qs" -c nacre ipc call nacre mode "$1" ;;`
- `nacre/shell-tools/control.sh:31` — `exec "$HOME/.local/share/nacre/shell/bin/qs" -c nacre ipc call nacre mode clipboard`
- `nacre/shell-tools/control.sh:33` — `session) exec "$HOME/.local/share/nacre/shell/bin/qs" -c nacre ipc call nacre session ;;`
- `nacre/shell-tools/control.sh:34` — `hide) exec "$HOME/.local/share/nacre/shell/bin/qs" -c nacre ipc call nacre hide ;;`
- `nacre/shell-tools/control.sh:35` — `launcher) exec "$HOME/.local/share/nacre/shell/bin/qs" -c nacre ipc call nacre launcher "" ;;`
- `nacre/shell-tools/control.sh:36` — `toggle) exec "$HOME/.local/share/nacre/shell/bin/qs" -c nacre ipc call drawers toggle dashboard ;;`
- `nacre/shell-tools/lock-info.py:25` — `def ipc(target, action):`
- `nacre/shell-tools/lock-info.py:60` — `data = json.loads(ipc("lockWidgets", "state"))`
- `nacre/shell-tools/lock-info.py:295` — `ipc("mpris", "playPause" if a.kind == "toggle" else a.kind)`
- `nacre/shell-tools/menu-cli.py:194` — `def ipc(target, *args):`
- `nacre/shell-tools/menu-cli.py:241` — `ipc("menuPrompt", "open", ident)`
- `nacre/shell-tools/menu-cli.py:259` — `ipc("menuPrompt", "cancel", ident)`
- `nacre/shell-tools/menu-cli.py:313` — `ipc("nacre", "menu", args.route, args.value)`
- `nacre/shell-tools/session-watch.py:32` — `desktop = run([str(qs), "-c", "nacre", "ipc", "call", "nacre", "state"])`
