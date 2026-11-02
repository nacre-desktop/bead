# B0213 — Launcher readability, plugins and action audit

Status: candidate; remote CI, refreshed goldens and paired visual review pending. One Nacre engine gap is an N-unit proposal below. No live session was used.

## Changes and root cause

The results grid fixed every row at 55 px and its description at 16 px. Text still wrapped, so a second line overflowed the reserved box and row. There is no launcher gradient/fade mask over that text. Results reuse the existing bounded-spacer virtual list (including the engine total beyond a loaded prefix), avoiding the 20,000 px single-dimension limit for tall lists. They reserve a single ellipsized title and up to two measured description lines, preserving explicit newlines and adding an ellipsis beyond the limit. At the default font sizes the row is 74 px (21 + 2×17 + 3 + 16 padding). Grid labels use one ellipsized line; grid height also grows for larger font settings. Each text line is separately measured and painted, with no text overlay.

The general `ui.text_line_height(family,size)` primitive exposes the renderer's 1.4× line pitch after the host's font-size adjustment; launcher row height follows it. This prevents a larger text setting from reintroducing the cutoff. No renderer/slide change or engine-specific framework primitive was added.

Plugins is a real rail entry (`extension` icon) that writes `>plugin `, not a fabricated engine route. Its existing install (git URL or absolute folder), installed/access list, enable/disable, check and update actions remain `plugins.do`; the list scrolls and long copy is bounded. Widgets opens the existing on-demand Widgets app; app routing runs off the UI thread. The rail scrolls so all modes remain reachable on short panels. No catalog/ranking/provider implementation was copied.

Catalog and built-in provider icons are Material Symbols, but result rows previously sent them to the desktop image-theme resolver. A shell-side table validates the symbol names against upstream codepoints and renders their ligatures with the pinned font; desktop application/custom icons retain their theme/path resolver. The table is a subset of [upstream codepoints](https://raw.githubusercontent.com/google/material-design-icons/737e3324305806514d7909874fa1818ae1808232/variablefont/MaterialSymbolsRounded%5BFILL%2CGRAD%2Copsz%2Cwght%5D.codepoints), matching Nacre's pinned font revision. Apache-2.0 attribution is retained; the existing font license is in `crates/bead-test/fonts/Apache-2.0.txt`.

Three real dynamic extension gaps are fixed: panel opens omit the empty action field (which the helper treated as an invalid command ID); `software-extension-action` uses the existing helper `software-library open` command, and `software-extension-toggle` reads the helper's authoritative current enabled state before enable/disable. The pinned UI activation schema omits checkbox state; the adapter does not infer it from translated labels or stale UI data.

## Audit scope and evidence

Source audit: Nacre origin/main `816957914b3c95872f942a9bd6d7bb0b4ac8a64e`, `nacre/shell-tools/launcher-menu.json`, `crates/nacre-menu/src/{providers,content,software,browser}.rs`, helper dispatchers `menu.rs`, `launcher_workflows.rs`, `software_library.rs`, and `nacre/shell-tools/control.sh`. Bead dispatch: `launcher_actions.rs`, `launcher_form.rs`, `commands.rs`, `key_routes.rs`, `app.rs`, `plugins.rs`. The older catalog in B0197 matches the current static action IDs; this unit refreshes its provenance and adds navigation/icons and the missing dynamic operations to the existing test fixture.

“Supported” below means a source-backed target exists, not that runtime execution passed in this unit. The provider rows are representative operation contracts, not an enumeration of installed apps, user commands, extension manifests or clipboard contents. Arbitrary external application/custom icons still depend on their installed theme/files; no static source audit can certify those files. Existing capabilities and availability checks remain authoritative; missing packages/devices do not become fake successes. Informational rows intentionally do not launch anything.

Recording already routes Record area/screen through the sound-choice form to `recorder.start {mode,audio}`; Stop uses `recorder.stop`. Capture commands run after launcher close. Nacre's PATH `nacre-shell` wrapper owns AI/Brain verbs; Bead installs its binary under the separate Bead library path and exposes `bead-shell`, so these are not calls to Bead's option parser.

## Entry table

| Entry | Route / dispatch target | Icon | Status before → after |
|---|---|---|---|
| home · Home | menu.query route `home` | `home` | supported; theme lookup → supported; Symbols |
| apps · All apps | menu.query route `apps` / apps | `apps` | supported; theme lookup → supported; Symbols |
| actions · Actions | menu.query route `actions` | `bolt` | supported; theme lookup → supported; Symbols |
| style · Style | menu.query route `style` | `palette` | supported; theme lookup → supported; Symbols |
| setup · Setup | menu.query route `setup` | `tune` | supported; theme lookup → supported; Symbols |
| software · Software | menu.query route `software` | `download` | supported; theme lookup → supported; Symbols |
| health · Health | menu.query route `health` | `build` | supported; theme lookup → supported; Symbols |
| learn · Learn | menu.query route `learn` | `help` | supported; theme lookup → supported; Symbols |
| power · Power | menu.query route `power` | `power_settings_new` | supported; theme lookup → supported; Symbols |
| actions.capture · Capture | menu.query route `actions.capture` | `screenshot` | supported; theme lookup → supported; Symbols |
| actions.clipboard · Clipboard history | menu.query route `actions.clipboard` / clipboard | `content_paste` | supported; theme lookup → supported; Symbols |
| actions.emoji · Emoji | menu.query route `actions.emoji` / emoji | `mood` | supported; theme lookup → supported; Symbols |
| actions.windows · Windows | menu.query route `actions.windows` / windows | `view_quilt` | supported; theme lookup → supported; Symbols |
| actions.reminders · Reminders | menu.query route `actions.reminders` / timers | `timer` | supported; theme lookup → supported; Symbols |
| actions.assistant · Nacre AI and Brain | menu.query route `actions.assistant` | `neurology` | supported; theme lookup → supported; Symbols |
| actions.controls · Quick controls | menu.query route `actions.controls` | `toggle_on` | supported; theme lookup → supported; Symbols |
| style.wallpapers · Wallpapers | menu.query route `style.wallpapers` / wallpapers | `wallpaper` | supported; theme lookup → supported; Symbols |
| style.personality · Orient personality | menu.query route `style.personality` | `palette` | supported; theme lookup → supported; Symbols |
| style.background · Background tint | menu.query route `style.background` | `gradient` | supported; theme lookup → supported; Symbols |
| style.appearance · Light and dark | menu.query route `style.appearance` | `contrast` | supported; theme lookup → supported; Symbols |
| style.accents · Accent choice | menu.query route `style.accents` / accents | `color_lens` | supported; theme lookup → supported; Symbols |
| style.fonts · Interface font | menu.query route `style.fonts` / fonts | `text_fields` | supported; theme lookup → supported; Symbols |
| style.mono-fonts · Terminal font | menu.query route `style.mono-fonts` / fonts | `terminal` | supported; theme lookup → supported; Symbols |
| setup.wifi-networks · Wi-Fi networks | menu.query route `setup.wifi-networks` / wifi | `wifi` | supported; theme lookup → supported; Symbols |
| setup.bluetooth-devices · Bluetooth devices | menu.query route `setup.bluetooth-devices` / bluetooth | `bluetooth` | supported; theme lookup → supported; Symbols |
| software.discover · Apps | menu.query route `software.discover` / software | `apps` | supported; theme lookup → supported; Symbols |
| software.packages · Search packages | menu.query route `software.packages` / packages | `search` | supported; theme lookup → supported; Symbols |
| software.installed · Installed packages | menu.query route `software.installed` / installed | `inventory_2` | supported; theme lookup → supported; Symbols |
| software.development · Developer environments | menu.query route `software.development` / environments | `code` | supported; theme lookup → supported; Symbols |
| software.tui · Terminal apps | menu.query route `software.tui` / tui-apps | `terminal` | supported; theme lookup → supported; Symbols |
| software.webapps · Web apps | menu.query route `software.webapps` / webapps | `public` | supported; theme lookup → supported; Symbols |
| learn.shortcuts · Keyboard shortcuts | menu.query route `learn.shortcuts` / keys | `keyboard` | supported; theme lookup → supported; Symbols |
| actions.controls.profile · Power profile | menu.query route `actions.controls.profile` | `battery_full` | supported; theme lookup → supported; Symbols |
| actions.media · Media and audio | menu.query route `actions.media` / audio | `volume_up` | supported; theme lookup → supported; Symbols |
| actions.saved · Saved launcher items | menu.query route `actions.saved` | `bookmark` | supported; theme lookup → supported; Symbols |
| actions.dictation · Voice typing | menu.query route `actions.dictation` | `mic` | supported; theme lookup → supported; Symbols |
| software.themes · Themes | menu.query route `software.themes` / software-themes | `palette` | supported; theme lookup → supported; Symbols |
| software.extensions · Extensions | menu.query route `software.extensions` / software-extensions | `extension` | supported; theme lookup → supported; Symbols |
| software.discover.ai · AI | menu.query route `software.discover.ai` / software-ai | `smart_toy` | supported; theme lookup → supported; Symbols |
| health.fixits · Fix a problem | menu.query route `health.fixits` / health-fixits | `build` | supported; theme lookup → supported; Symbols |
| setup.system · System setup | menu.query route `setup.system` / system-setup | `settings_suggest` | supported; theme lookup → supported; Symbols |
| setup.hardware · Hardware controls | menu.query route `setup.hardware` / system-hardware | `devices` | supported; theme lookup → supported; Symbols |
| health.speed · Speed tests | menu.query route `health.speed` / system-speed | `speed` | supported; theme lookup → supported; Symbols |
| actions.capture.area · Screenshot area | helper launcher-workflows capture capture-area | `crop_free` | supported; theme lookup → supported; Symbols |
| actions.capture.screen · Screenshot screen | helper launcher-workflows capture capture-screen | `screenshot_monitor` | supported; theme lookup → supported; Symbols |
| actions.capture.window · Screenshot window | helper launcher-workflows capture capture-window | `window` | supported; theme lookup → supported; Symbols |
| actions.capture.record · Record an area | form → recorder.start region | `videocam` | supported; theme lookup → supported; Symbols |
| actions.capture.record-screen · Record screen | form → recorder.start screen | `screen_share` | supported; theme lookup → supported; Symbols |
| actions.capture.stop · Stop recording | recorder.stop | `stop_circle` | supported; theme lookup → supported; Symbols |
| actions.capture.text · Read text from screen | helper menu-backend tool capture-text | `text_fields` | supported; theme lookup → supported; Symbols |
| actions.capture.qr · Read a QR code | helper launcher-workflows qr | `qr_code` | supported; theme lookup → supported; Symbols |
| actions.capture.color · Pick a screen color | helper menu-backend tool color-picker | `colorize` | supported; theme lookup → supported; Symbols |
| actions.calculator · Calculator | query prefix `= ` | `calculate` | supported; theme lookup → supported; Symbols |
| actions.reminders.new · Set a reminder | launcher form reminder → helper (B0199) | `add_alarm` | supported; theme lookup → supported; Symbols |
| actions.reminders.focus · Focus for 25 minutes | helper menu-backend tool focus-timer | `hourglass_top` | supported; theme lookup → supported; Symbols |
| actions.assistant.ai · Nacre AI | nacre-shell tasks → helper desktop-actions | `neurology` | supported; theme lookup → supported; Symbols |
| actions.assistant.new · Nacre AI: New chat | nacre-shell new → helper desktop-actions | `neurology` | supported; theme lookup → supported; Symbols |
| actions.assistant.resume · Nacre AI: Resume chat | nacre-shell resume → helper desktop-actions | `neurology` | supported; theme lookup → supported; Symbols |
| actions.assistant.chats · Nacre AI: Browse chats | kitty → nacre-ai window sessions | `neurology` | supported; theme lookup → supported; Symbols |
| actions.assistant.brain · Nacre Brain | nacre-shell brain → helper brain-control | `edit_note` | supported; theme lookup → supported; Symbols |
| actions.assistant.capture · Nacre Brain: Capture note | nacre-shell capture → helper brain-control | `edit_note` | supported; theme lookup → supported; Symbols |
| actions.assistant.search · Search Nacre Brain | query prefix `# ` | `search` | supported; theme lookup → supported; Symbols |
| actions.assistant.ask · Ask Nacre AI | query prefix `? ` | `chat` | supported; theme lookup → supported; Symbols |
| actions.controls.dnd · Do not disturb | ControlCenter toggle dnd → published daemon method | `notifications_off` | supported; theme lookup → supported; Symbols |
| actions.controls.night-light · Night light | ControlCenter toggle night-light → published daemon method | `bedtime` | supported; theme lookup → supported; Symbols |
| actions.controls.stay-awake · Stay awake | ControlCenter toggle stay-awake → published daemon method | `coffee` | supported; theme lookup → supported; Symbols |
| actions.controls.mute · Mute speakers | ControlCenter toggle mute → published daemon method | `volume_off` | supported; theme lookup → supported; Symbols |
| actions.controls.mic-mute · Mute microphone | ControlCenter toggle mic-mute → published daemon method | `mic_off` | supported; theme lookup → supported; Symbols |
| actions.controls.wifi · Wi-Fi | ControlCenter toggle wifi → published daemon method | `wifi` | supported; theme lookup → supported; Symbols |
| actions.controls.bluetooth · Bluetooth | ControlCenter toggle bluetooth → published daemon method | `bluetooth` | supported; theme lookup → supported; Symbols |
| style.wallpapers.picker · Wallpaper picker | nacre mode wallpaper | `wallpaper` | supported; theme lookup → supported; Symbols |
| style.wallpapers.next · Next wallpaper | wallpaper.next | `skip_next` | supported; theme lookup → supported; Symbols |
| style.personality.natural · Natural | helper wallpaper-media theme (personality: natural) | `palette` | supported; theme lookup → supported; Symbols |
| style.personality.pigment · Pigment | helper wallpaper-media theme (personality: pigment) | `palette` | supported; theme lookup → supported; Symbols |
| style.personality.harmony · Harmony | helper wallpaper-media theme (personality: harmony) | `palette` | supported; theme lookup → supported; Symbols |
| style.personality.pop · Pop | helper wallpaper-media theme (personality: pop) | `palette` | supported; theme lookup → supported; Symbols |
| style.personality.mist · Mist | helper wallpaper-media theme (personality: mist) | `palette` | supported; theme lookup → supported; Symbols |
| style.personality.vivid · Vivid | helper wallpaper-media theme (personality: vivid) | `palette` | supported; theme lookup → supported; Symbols |
| style.personality.pearl · Pearl | helper wallpaper-media theme (personality: pearl) | `palette` | supported; theme lookup → supported; Symbols |
| style.personality.tide · Tide | helper wallpaper-media theme (personality: tide) | `palette` | supported; theme lookup → supported; Symbols |
| style.background.accent · Accent tint | helper wallpaper-media theme (background: accent) | `palette` | supported; theme lookup → supported; Symbols |
| style.background.wallpaper · Wallpaper shadows | helper wallpaper-media theme (background: wallpaper) | `image` | supported; theme lookup → supported; Symbols |
| style.appearance.dark · Dark | appearance.set_mode dark | `dark_mode` | supported; theme lookup → supported; Symbols |
| style.appearance.light · Light | appearance.set_mode light | `light_mode` | supported; theme lookup → supported; Symbols |
| style.appearance.auto · Auto | appearance.set_mode auto | `light_mode` | supported; theme lookup → supported; Symbols |
| style.animations · Animations | settings.get/set desktop animations | `animation` | supported; theme lookup → supported; Symbols |
| style.blur · Background blur | settings.get/set desktop blur | `blur_on` | supported; theme lookup → supported; Symbols |
| style.shadow · Window shadows | settings.get/set desktop shadow | `shadow` | supported; theme lookup → supported; Symbols |
| style.colors · Nacre Colors | colorsApp.open | `palette` | supported; theme lookup → supported; Symbols |
| setup.appearance · Appearance | nacre settings appearance (registry alias) | `palette` | supported; theme lookup → supported; Symbols |
| setup.desktop · Desktop and input | nacre settings desktop (registry alias) | `desktop_windows` | supported; theme lookup → supported; Symbols |
| setup.displays · Displays | nacre settings displays (registry alias) | `monitor` | supported; theme lookup → supported; Symbols |
| setup.sound · Sound | nacre settings sound (registry alias) | `volume_up` | supported; theme lookup → supported; Symbols |
| setup.network · Network | nacre settings network (registry alias) | `wifi` | supported; theme lookup → supported; Symbols |
| setup.bluetooth · Bluetooth | nacre settings bluetooth (registry alias) | `bluetooth` | supported; theme lookup → supported; Symbols |
| setup.notifications · Notifications | nacre settings notifications (registry alias) | `notifications` | supported; theme lookup → supported; Symbols |
| setup.workflows · Workspaces and apps | nacre settings workflows (registry alias) | `workspaces` | supported; theme lookup → supported; Symbols |
| setup.lock · Lock screen and idle | nacre settings lock (registry alias) | `lock` | supported; theme lookup → supported; Symbols |
| setup.time · Date and time | nacre settings time (registry alias) | `schedule` | supported; theme lookup → supported; Symbols |
| setup.ai · Nacre AI preferences | nacre settings ai (registry alias) | `neurology` | supported; theme lookup → supported; Symbols |
| setup.shortcuts · Edit personal shortcuts | kitty → helper menu-backend config shortcuts | `keyboard` | supported; theme lookup → supported; Symbols |
| setup.defaults · Default applications | kitty → helper menu-backend config defaults | `apps` | supported; theme lookup → supported; Symbols |
| software.development.mise · Install mise | confirmation → helper software-package install mise | `download` | supported; theme lookup → supported; Symbols |
| software.tui.new · Create a TUI app | launcher form tui → helper (B0199) | `add` | supported; theme lookup → supported; Symbols |
| software.webapps.new · Create a web app | launcher form webapp → helper (B0199) | `add` | supported; theme lookup → supported; Symbols |
| health.updates · System updates | helper menu-backend tool updates | `system_update` | supported; theme lookup → supported; Symbols |
| health.nacre · Nacre maintenance | nacre settings maintenance (registry alias) | `build` | supported; theme lookup → supported; Symbols |
| health.check · Check the desktop | helper maintenance check | `health_and_safety` | supported; theme lookup → supported; Symbols |
| health.status · Current health | view-only information | `monitor_heart` | supported; theme lookup → supported; Symbols |
| health.support-report · Copy support report | helper menu-backend tool support-report | `content_copy` | supported; theme lookup → supported; Symbols |
| health.logs · Desktop logs | helper menu-backend tool logs | `article` | supported; theme lookup → supported; Symbols |
| health.restart · Restart the desktop shell | confirmation → helper maintenance restart | `restart_alt` | supported; theme lookup → supported; Symbols |
| health.rollback · Review a rollback | nacre settings maintenance (registry alias) | `restore` | supported; theme lookup → supported; Symbols |
| learn.welcome · Nacre Welcome | welcomeApp.open | `waving_hand` | supported; theme lookup → supported; Symbols |
| learn.nacre · Nacre guide | view-only information | `help` | supported; theme lookup → supported; Symbols |
| learn.hyprland · Hyprland manual | xdg-open https://wiki.hypr.land/ | `public` | supported; theme lookup → supported; Symbols |
| learn.arch · Arch Linux guide | xdg-open https://wiki.archlinux.org/ | `public` | supported; theme lookup → supported; Symbols |
| learn.quickshell · Quickshell documentation | xdg-open https://quickshell.org/docs/ | `public` | supported; theme lookup → supported; Symbols |
| learn.news · Nacre project | xdg-open https://github.com/nacre-desktop/nacre | `public` | supported; theme lookup → supported; Symbols |
| power.lock · Lock the desktop | session.request lock | `lock` | supported; theme lookup → supported; Symbols |
| power.suspend · Sleep | session view → session.request suspend (confirmation) | `bedtime` | supported; theme lookup → supported; Symbols |
| power.logout · Log out | session view → session.request logout (confirmation) | `logout` | supported; theme lookup → supported; Symbols |
| power.reboot · Restart | session view → session.request reboot (confirmation) | `restart_alt` | supported; theme lookup → supported; Symbols |
| power.poweroff · Shut down | session view → session.request poweroff (confirmation) | `power_settings_new` | supported; theme lookup → supported; Symbols |
| actions.files · Files | nacre-app files | `folder_open` | supported; theme lookup → supported; Symbols |
| actions.terminal · Terminal | nacre-app terminal | `terminal` | supported; theme lookup → supported; Symbols |
| actions.apps · All apps | nacre menu apps | `apps` | supported; theme lookup → supported; Symbols |
| actions.controls.profile.power-saver · Power saver | power.set_profile power-saver | `battery_full` | supported; theme lookup → supported; Symbols |
| actions.controls.profile.balanced · Balanced | power.set_profile balanced | `battery_full` | supported; theme lookup → supported; Symbols |
| actions.controls.profile.performance · Performance | power.set_profile performance | `battery_full` | supported; theme lookup → supported; Symbols |
| actions.media.play-pause · Play or pause | player.* active player | `play_arrow` | supported; theme lookup → supported; Symbols |
| actions.media.next · Next track | player.* active player | `skip_next` | supported; theme lookup → supported; Symbols |
| actions.media.previous · Previous track | player.* active player | `skip_previous` | supported; theme lookup → supported; Symbols |
| actions.find-files · Find files | query prefix `: ` | `folder` | supported; theme lookup → supported; Symbols |
| actions.saved.add-action · Add launcher action | launcher form action → helper (B0199) | `terminal` | supported; theme lookup → supported; Symbols |
| actions.saved.add-snippet · Save text snippet | launcher form snippet → helper (B0199) | `content_paste` | supported; theme lookup → supported; Symbols |
| actions.saved.add-search · Add named web search | launcher form search → helper (B0199) | `public` | supported; theme lookup → supported; Symbols |
| actions.saved.add-bookmark · Save bookmark | launcher form bookmark → helper (B0199) | `bookmark` | supported; theme lookup → supported; Symbols |
| actions.saved.add-workspace · Save workspace recipe | launcher form workspace → helper (B0199) | `workspaces` | supported; theme lookup → supported; Symbols |
| actions.saved.add-roots · Choose file search folders | launcher form roots → helper (B0199) | `folder` | supported; theme lookup → supported; Symbols |
| actions.dictation.toggle · Start or stop voice typing | literal argv process launch (app provider fixture) | `mic` | supported; theme lookup → supported; Symbols |
| actions.dictation.setup · Configure voice typing | literal argv process launch (app provider fixture) | `settings_voice` | supported; theme lookup → supported; Symbols |
| actions.share · Send or receive with LocalSend | literal argv process launch (app provider fixture) | `share` | supported; theme lookup → supported; Symbols |
| software.themes.import · Import a theme | launcher form software-theme-import → helper (B0199) | `download` | supported; theme lookup → supported; Symbols |
| software.themes.create · Create a theme | nacre settings colors (registry alias) | `palette` | supported; theme lookup → supported; Symbols |
| software.extensions.import · Add an extension | launcher form software-extension-import → helper (B0199) | `add` | supported; theme lookup → supported; Symbols |
| software.packages.review-policy · AUR package review policy | nacre settings system (registry alias) | `policy` | supported; theme lookup → supported; Symbols |
| actions.bring-visitors · Bring visitors here | helper monitor-workspaces bring | `workspaces` | supported; theme lookup → supported; Symbols |
| calculation · calculation | wl-copy text | `content_copy` | supported; theme lookup → supported; Symbols |
| actions.emoji.x · actions.emoji.x | wl-copy text | `content_copy` | supported; theme lookup → supported; Symbols |
| actions.clipboard.1 · actions.clipboard.1 | clipboard.copy | `content_paste` | supported; theme lookup → supported; Symbols |
| apps.audio.output-1 · apps.audio.output-1 | audio.set_default | `speaker` | supported; theme lookup → supported; Symbols |
| style.wallpapers.abc · style.wallpapers.abc | wallpaper: /home/u/Pictures/Wallpapers/a.png | `wallpaper` | supported; theme lookup → supported; Symbols |
| style.accents.choice-0 · style.accents.choice-0 | helper wallpaper-media theme (accent: #7a5af8) | `palette` | supported; theme lookup → supported; Symbols |
| actions.windows.1 · actions.windows.1 | window.focus | `window` | supported; theme lookup → supported; Symbols |
| actions.windows.2 · actions.windows.2 | window.focus | `window` | supported; theme lookup → supported; Symbols |
| setup.network.ap · setup.network.ap | nacre settings network (registry alias) | `bolt` | supported; theme lookup → supported; Symbols |
| setup.bluetooth.dev · setup.bluetooth.dev | nacre settings bluetooth (registry alias) | `bolt` | supported; theme lookup → supported; Symbols |
| learn.shortcuts.binding-0 · learn.shortcuts.binding-0 | view-only information | `info` | supported; theme lookup → supported; Symbols |
| actions.reminders.unit · actions.reminders.unit | helper menu-backend cancel-reminder | `timer` | supported; theme lookup → supported; Symbols |
| software.webapps.x · software.webapps.x | nacre-menu webapp-launch | `public` | supported; theme lookup → supported; Symbols |
| software.installed.x · software.installed.x | kitty → helper menu-backend remove | `inventory_2` | supported; theme lookup → supported; Symbols |
| software.packages.x · software.packages.x | kitty → helper software-package install | `download` | supported; theme lookup → supported; Symbols |
| style.fonts.inter · style.fonts.inter | settings.get/set or helper theme bundle (font: Inter) | `text_fields` | supported; theme lookup → supported; Symbols |
| style.mono-fonts.jb · style.mono-fonts.jb | settings.get/set or helper theme bundle (mono-font: JetBrains Mono) | `terminal` | supported; theme lookup → supported; Symbols |
| software.development.node-install · software.development.node-install | kitty → helper software-environments run | `terminal` | supported; theme lookup → supported; Symbols |
| software.tui.htop · software.tui.htop | helper software-library tui-launch | `terminal` | supported; theme lookup → supported; Symbols |
| software.tui.htop-remove · software.tui.htop-remove | helper software-library tui-remove | `terminal` | supported; theme lookup → supported; Symbols |
| software.ai.claude · software.ai.claude | kitty literal terminalCommand | `terminal` | supported; theme lookup → supported; Symbols |
| software.discover.site · software.discover.site | helper menu-backend webapp-create | `public` | supported; theme lookup → supported; Symbols |
| software.themes.preset · software.themes.preset | settings.get/set or helper theme bundle (software-theme: catppuccin) | `palette` | supported; theme lookup → supported; Symbols |
| software.themes.bundle · software.themes.bundle | settings.get/set or helper theme bundle (software-theme: bundle-1) | `palette` | supported; theme lookup → supported; Symbols |
| software.extensions.x · software.extensions.x | helper software-library install-builtin x-ext | `extension` | supported; theme lookup → supported; Symbols |
| software.extensions.y · software.extensions.y | helper software-library open y-ext / extensionAction | `extension` | supported; theme lookup → supported; Symbols |
| software.extensions.theme · software.extensions.theme | helper software-library extension-theme z-ext | `palette` | supported; theme lookup → supported; Symbols |
| system.speed.x · system.speed.x | helper software-library system-open | `build` | supported; theme lookup → supported; Symbols |
| actions.palette.brain · actions.palette.brain | DesktopActions adapter: brain-query notes | `settings` | supported; theme lookup → supported; Symbols |
| nacre-action:colors: · nacre-action:colors: | DesktopActions adapter: colors  | `settings` | supported; theme lookup → supported; Symbols |
| nacre-action:preset:focused · nacre-action:preset:focused | DesktopActions adapter: preset focused | `settings` | supported; theme lookup → supported; Symbols |
| nacre-action:toggle-setting:blur · nacre-action:toggle-setting:blur | DesktopActions adapter: toggle-setting blur | `settings` | supported; theme lookup → supported; Symbols |
| nacre-action:dark: · nacre-action:dark: | DesktopActions adapter: dark  | `settings` | supported; theme lookup → supported; Symbols |
| nacre-action:overview: · nacre-action:overview: | DesktopActions adapter: overview  | `settings` | supported; theme lookup → supported; Symbols |
| files.1 · files.1 | xdg-open (absolute engine file path) | `folder_open` | supported; theme lookup → supported; Symbols |
| actions.assistant.search.n · actions.assistant.search.n | xdg-open (absolute engine note path) | `description` | supported; theme lookup → supported; Symbols |
| actions.search-web · actions.search-web | nacre-app browser / engine webUrl | `public` | supported; theme lookup → supported; Symbols |
| actions.search-web-old · actions.search-web-old | nacre-app browser / engine webUrl | `public` | supported; theme lookup → supported; Symbols |
| actions.assistant.question · actions.assistant.question | ai.chat.new/send → chat app | `chat` | supported; theme lookup → supported; Symbols |
| actions.saved.engine · actions.saved.engine | helper launcher-workflows activate | `workspaces` | supported; theme lookup → supported; Symbols |
| provider-extension-panel · Open extension panel | helper software-library open (no action field) | `extension` | invalid empty action → panel open payload |
| provider-extension-toggle · Enable extension | helper software-library load → enable/disable (current state) | `extension` | unsupported → existing helper route |
| provider-extension-action · Extension command | helper software-library open user.fixture / extensionAction | `extension` | unsupported → existing helper route |
| provider-software-open · Open installed software | N-unit proposal: daemon activates indexed desktopId (not published to UI) | `download` | unsupported → N-unit proposal; remains unavailable |
| Widgets | launcher.app widgets → app::route / bead.window open | `widgets` | missing → existing Widgets editor app |
| Plugins | navigate plugins → `>plugin ` | `extension` | hidden prefix only → discoverable mode |
| Plugin Install | plugins.do install\|SOURCE → bead-ui plugin installer | `extension` (mode) | working prefix mode → discoverable mode |
| Plugin access / Enable / Disable | plugins.do enable\|ID / disable\|ID | `extension` (mode) | clipped access → bounded readable access |
| Check plugin updates / Update | plugins.do check / update\|ID → existing git updater | `extension` (mode) | working prefix mode → discoverable mode |

## N-unit proposal and limits

**Installed software “Open” (`software-open`):** current Nacre emits a package value plus `desktopId`, but the published `MenuUiRow` drops `desktopId`. There is no helper “open package's indexed app” route. Bead cannot guess a desktop ID, substitute a search, or launch the package name. Proposed N-unit: let daemon `menu.activate` handle `software-open` against its existing desktop index, using its normal application activation path; alternatively publish the exact desktop activation contract. This needs no second index in Bead. The current opaque result contract does not expose operation data to Bead's display projection, so selective source-side removal/hiding also belongs in that N-unit. The negative fixture makes this exception explicit; it is not counted as working. No Nacre files changed here.

The Widgets/Plugins rail entries are intentionally Bead-owned because the shared Nacre catalog has neither. All other audited static entries have existing routes; none was removed. Clipboard secondary actions and extension context menus remain the pre-existing B0197 gap, outside the engine's activation rows audited here. No temporary replacement/duplicate was introduced. New UI state is lazy scroll scalars and shell module data only; budget ≤1 MiB always-on unreclaimable, actual memory unmeasured. No performance or memory clearance is claimed.

## Validation handoff

Both authorized remote-host probes failed (unreachable/timed out), so no build, test, nested capture or CI ran on the laptop. Single-file Rust formatting/checks use edition 2024, the repository edition; the suggested edition 2021 cannot parse its existing let chains. Luau analyzer was not on PATH.

Added tests cover explicit-newline/wrapped two-line descriptions, row/clip bounds, text width/line pitch/opacity and ellipsis; Plugins rail → prefix and Widgets target; all audited built-in icon names resolving through the actual codepoint table; every supported action's dispatch plan; extension state/command routing and the explicit software-open proposal. Existing B0208 tests cover install/access and plugin host operations.

Lead must run the complete `.github/workflows/ci.yml` job in its prescribed Ubuntu 24.04 CI container, including fmt, warnings-denied clippy, workspace/trace tests, renderer feature checks, generated Luau schema/analyzer, all Python/proof/packaging/service/video tests and all cargo-deny graphs. Focus first on `cargo test -p nacre-shell --locked -j2 launcher_pixels_tests::`, `launcher_actions_tests::`, `plugins_tests::`, and existing app/command-route tests. The launcher row/grid goldens and any launcher full-scene goldens affected by the new rail must be regenerated and visually reviewed remotely (`BEAD_UPDATE_GOLDENS=1`), then rerun with updates off; existing exact comparisons were not weakened. Matched private QML/Bead open/hover/pressed/modes captures and the lead's live acceptance remain pending; known intended differences are the readable larger result rows and the two new rail entries. No screenshots are claimed.
