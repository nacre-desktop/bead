# B0197: Every launcher action works on Bead

Status: candidate (B0199 closed the form, recording, search-engine and confirmation gaps). The owner hit "Record screen … not supported on Bead". Cause: the daemon's menu engine hands every non-app row to the UI provider (`ui.menu.activate`), and Bead's provider (`ui_provider.rs`) accepted only five operations (`copy`, `copy-clip`, `command`, `power`, `route`). Everything else, about 90% of the Actions, Style, Setup, Health, Learn and Software rows, was refused with "Launcher action is not supported by this UI".

## What changed
- `crates/nacre-shell/src/launcher_actions.rs` (new): the port of QML `NacreMenuContext.execute` and `DesktopActions.execute`. Each operation the engine emits maps to one of three things:
  - a published daemon method: `recorder.start/stop`, `appearance.set_mode`, `wallpaper.next`, `wallpaper.activate`, `power.set_profile`, `audio.set_default`, `window.focus`, `player.*` on the active `dashboard.player`, `settings.get/set/desktop`, `ai.chat.new/send`, `clipboard.copy`, `session.request`;
  - a Bead route: Settings deep links, the Colors/Welcome apps, overview/palette/keybindings panels, the session view, launcher routes, the AI sidebar, and view-only replies (scope prefixes, `= ` calculator, info rows);
  - the same helper command QML ran (`nacre-helper menu-backend|launcher-workflows|software-library|wallpaper-media|maintenance …`, the kitty terminals, `xdg-open`, `nacre-app browser`).
  Toggles (`dnd`, night light, stay awake, mute, mic, Wi-Fi, Bluetooth) reuse `ControlCenter`'s toggle on a short-lived thread with view leases, so the read and the published call are the control centre's own. Interactive tools (capture, picker, recording) start 320 ms after the reply, as QML's `captureDelay` did, so the launcher has closed. `keep_open` follows QML `keepOpen` (toggles, styles, power profiles, media, outputs and timers keep the launcher open and it re-queries). Unknown operations are still refused with the published message; known ones with a bad operand fail validation (http(s) only for URLs, absolute paths for files, no `..` in ids).
- `ui_provider.rs`: transport only; the mapping and its tests moved out (+6 −235 lines there).
- `settings.rs` `page_for`: Settings deep links resolve the registry's `aliases` (`desktop`, `lock`, `time`, `ai`, `maintenance`, `workflows`), which the launcher's Setup rows use. Before, those opened the default page.
- Screen recording: after the B0199 sound choice, Record (region) and Record screen start the daemon recorder (the control centre's path; it refuses a second recording); Stop recording calls `recorder.stop`. Output is the helper's (`menu-backend tool record-start`), so files land in `~/Videos/Recordings` like QML.

## Tests
- `launcher_actions_tests::every_visible_launcher_action_resolves_to_a_supported_route`: table-driven over `crates/nacre-shell/tests/launcher-actions.json` (the whole Nacre catalogue at a73934c4, 105 rows, plus one row per `nacre-menu` provider operation, 40 rows). Every row must plan; Shell routes must pass `commands::validate`, Settings pages must exist (or alias onto one), daemon methods must be in the published list, toggles must be control-centre sources; every operation in `KNOWN` must have a fixture row. The only refusals allowed are the five form gaps below.
- Unit tests for recording/capture timing, view rows, toggles, the web search URL, and fail-closed operands; the published protocol transcript test moved here unchanged.

## Gaps: closed by B0199 unless listed here
B0197 left these. B0199 (with Nacre N0100) closed the first six; the rest remain.
- Closed: inline forms. Set a reminder, Create a TUI app, Create a web app, Import theme and Import extension open an inline form (`shell/ui/components/form.luau`, fields in `launcher.form.v1..v4`) that submits to the helper commands QML used (`menu-backend reminder|webapp-create`, `software-library tui-create|theme-import|import`). Validation matches QML (minutes 0.1-1440 and a message; name and http(s) URL; a command line and a known terminal). A helper answer (`message` or `error`) is shown in the launcher; errors stay in the form.
- Closed: recording sound. Record (region) and Record screen open a sound choice (none / system / system + microphone) and start the daemon recorder with `recorder.start {mode, audio}` after the launcher has closed. The daemon's `audio` is new (N0100); absent means no sound, so an older daemon still records.
- Closed: bar recording indicator. While `controls.recorder.recording` is true the bar shows QML's red stop button with the elapsed time (`shell/bar/recording.luau`, before the tray); clicking calls `recorder.stop`. Idle cost is one `controls` topic lease on the bar and no ticking: the indicator thread exists only while a recording runs (`bar_recording.rs`).
- Closed: web search uses the owner's engine. The engine's `search-web` rows now carry `webUrl` (N0100); Bead opens it and keeps DuckDuckGo only for an older daemon.
- Closed: confirmation for Restart shell and Install mise. Both open a confirmation form (the row's own question, Cancel and Continue) before the terminal runs. Power rows still confirm in the session view.
- Still open: saved actions/snippets/searches/bookmarks/workspaces/roots open their Settings page instead of an inline form.
- Still open: Brain: `note` rows open the file with `xdg-open` and "Search Nacre Brain" opens the AI sidebar; Bead has no Brain explorer view.
- Still open: QML context-menu operations (`clip-pin`, `clip-delete`, `file-action`, `webapp-remove`, `workflow-remove`, extension update/remove/toggle) are never emitted by the engine as activations; Bead has no secondary-action menu.

## B0199 design
- A form row is answered by the provider with a view whose route is `form:<kind>` and whose query is the row's confirmation text (`launcher_actions::form`); `launcher::activation_changes` turns that into the `launcher.form.*` stores and never touches the engine route. `launcher_form.rs` owns the kinds, the starting values, validation and the backends; the view and its labels are Luau and i18n.
- Buttons are one host action, `launcher.form` (`submit`/`cancel`); Enter in a one-line field submits. A helper submit runs on its own thread and answers through the menu's wake callback; recording and the confirmations close the launcher at once and run after the 320 ms close delay (QML `captureDelay`).
- Removed: the B0197 `tool` arms for `record-start`/`record-screen`/`install-mise` and the direct `maintenance restart`; the rows now go through the form.
- Cost: the recording stop button adds a `controls` topic lease to the bar (daemon-side the same topic the control centre leases); no timer runs unless a recording does. Memory budget <= 1 MiB; measured value is in the B0199 report.

## Counts
145 rows: 13 worked before, 127 fixed by B0197, the last 5 (inline forms) fixed by B0199, 0 removed.

## Every action: before and after
| Row | Operation | Value | Before (35ac2f88) | After |
|---|---|---|---|---|
| actions.capture.area | tool | capture-area | not supported | helper `launcher-workflows capture` after close |
| actions.capture.screen | tool | capture-screen | not supported | helper `launcher-workflows capture` after close |
| actions.capture.window | tool | capture-window | not supported | helper `launcher-workflows capture` after close |
| actions.capture.record | tool | record-start | not supported | sound choice, then daemon `recorder.start` region (B0199) |
| actions.capture.record-screen | tool | record-screen | not supported | sound choice, then daemon `recorder.start` screen (B0199) |
| actions.capture.stop | tool | record-stop | not supported | daemon `recorder.stop` |
| actions.capture.text | tool | capture-text | not supported | helper `menu-backend tool` after close |
| actions.capture.qr | tool | capture-qr | not supported | helper `launcher-workflows qr` after close |
| actions.capture.color | tool | color-picker | not supported | helper `menu-backend tool` after close |
| actions.calculator | calculator |  | not supported | `= ` prefix |
| actions.reminders.new | form | reminder | not supported | inline form (B0199) |
| actions.reminders.focus | tool | focus-timer | not supported | helper `menu-backend tool focus-timer` |
| actions.assistant.ai | legacy | tasks | not supported | `nacre-shell tasks` |
| actions.assistant.new | legacy | new | not supported | `nacre-shell new` |
| actions.assistant.resume | legacy | resume | not supported | `nacre-shell resume` |
| actions.assistant.chats | legacy | load | not supported | kitty `nacre-ai window` |
| actions.assistant.brain | legacy | brain | not supported | `nacre-shell brain` |
| actions.assistant.capture | legacy | capture | not supported | `nacre-shell capture` |
| actions.assistant.search | scope | #  | not supported | search prefix |
| actions.assistant.ask | scope | ?  | not supported | search prefix |
| actions.controls.dnd | toggle | dnd | not supported | control-centre toggle: daemon `notifications.set_dnd` |
| actions.controls.night-light | toggle | night-light | not supported | control-centre toggle: daemon `night_light.set` |
| actions.controls.stay-awake | toggle | stay-awake | not supported | control-centre toggle: daemon `stay_awake.set` |
| actions.controls.mute | toggle | mute | not supported | control-centre toggle: daemon `audio.set_mute sink` |
| actions.controls.mic-mute | toggle | mic-mute | not supported | control-centre toggle: daemon `audio.set_mute source` |
| actions.controls.wifi | toggle | wifi | not supported | control-centre toggle: daemon `network.set_wifi` |
| actions.controls.bluetooth | toggle | bluetooth | not supported | control-centre toggle: daemon `bluetooth.set_power` |
| style.wallpapers.picker | route | wallpaper | works | wallpaper picker |
| style.wallpapers.next | next-wallpaper |  | not supported | daemon `wallpaper.next` |
| style.personality.natural | personality | natural | not supported | helper `wallpaper-media theme` |
| style.personality.pigment | personality | pigment | not supported | helper `wallpaper-media theme` |
| style.personality.harmony | personality | harmony | not supported | helper `wallpaper-media theme` |
| style.personality.pop | personality | pop | not supported | helper `wallpaper-media theme` |
| style.personality.mist | personality | mist | not supported | helper `wallpaper-media theme` |
| style.personality.vivid | personality | vivid | not supported | helper `wallpaper-media theme` |
| style.personality.pearl | personality | pearl | not supported | helper `wallpaper-media theme` |
| style.personality.tide | personality | tide | not supported | helper `wallpaper-media theme` |
| style.background.accent | background | accent | not supported | helper `wallpaper-media theme` |
| style.background.wallpaper | background | wallpaper | not supported | helper `wallpaper-media theme` |
| style.appearance.dark | palette-mode | dark | not supported | daemon `appearance.set_mode` |
| style.appearance.light | palette-mode | light | not supported | daemon `appearance.set_mode` |
| style.appearance.auto | palette-mode | auto | not supported | daemon `appearance.set_mode` |
| style.animations | setting-toggle | animations | not supported | desktop store flip (`settings.set`) |
| style.blur | setting-toggle | blur | not supported | desktop store flip (`settings.set`) |
| style.shadow | setting-toggle | shadow | not supported | desktop store flip (`settings.set`) |
| style.colors | legacy | colors | not supported | Colors app |
| setup.appearance | settings | appearance | not supported | Settings `appearance` |
| setup.desktop | settings | desktop | not supported | Settings `desktop` |
| setup.displays | settings | displays | not supported | Settings `displays` |
| setup.sound | settings | sound | not supported | Settings `sound` |
| setup.network | settings | network | not supported | Settings `network` |
| setup.bluetooth | settings | bluetooth | not supported | Settings `bluetooth` |
| setup.notifications | settings | notifications | not supported | Settings `notifications` |
| setup.workflows | settings | workflows | not supported | Settings `workflows` |
| setup.lock | settings | lock | not supported | Settings `lock` |
| setup.time | settings | time | not supported | Settings `time` |
| setup.ai | settings | ai | not supported | Settings `ai` |
| setup.shortcuts | config | shortcuts | not supported | kitty: helper `menu-backend config` |
| setup.defaults | config | defaults | not supported | kitty: helper `menu-backend config` |
| software.development.mise | tool | install-mise | not supported | confirmation (B0199), then kitty: `software-package install mise` |
| software.tui.new | form | tui | not supported | inline form (B0199) |
| software.webapps.new | form | webapp | not supported | inline form (B0199) |
| health.updates | tool | updates | not supported | kitty: helper `menu-backend tool updates` |
| health.nacre | settings | maintenance | not supported | Settings `maintenance` |
| health.check | maintenance | check | not supported | kitty: helper `maintenance` |
| health.status | info | health | not supported | stays selected |
| health.support-report | tool | support-report | not supported | helper `menu-backend tool support-report` |
| health.logs | tool | logs | not supported | kitty: helper `menu-backend tool logs` |
| health.restart | maintenance | restart | not supported | confirmation (B0199), then kitty: helper `maintenance` |
| health.rollback | settings | maintenance | not supported | Settings `maintenance` |
| learn.welcome | legacy | welcome | not supported | Welcome app |
| learn.nacre | info | guide | not supported | stays selected |
| learn.hyprland | url | https://wiki.hypr.land/ | not supported | xdg-open |
| learn.arch | url | https://wiki.archlinux.org/ | not supported | xdg-open |
| learn.quickshell | url | https://quickshell.org/docs/ | not supported | xdg-open |
| learn.news | url | https://github.com/nacre-desktop/nacre | not supported | xdg-open |
| power.lock | power | lock | works | daemon `session.request` lock |
| power.suspend | power | suspend | works (session view) | session view (confirms) |
| power.logout | power | logout | works (session view) | session view (confirms) |
| power.reboot | power | reboot | works (session view) | session view (confirms) |
| power.poweroff | power | poweroff | works (session view) | session view (confirms) |
| actions.files | legacy | files | not supported | `nacre-app files` |
| actions.terminal | legacy | terminal | not supported | `nacre-app terminal` |
| actions.apps | route | apps | works | launcher route |
| actions.controls.profile.power-saver | power-profile | power-saver | not supported | daemon `power.set_profile` |
| actions.controls.profile.balanced | power-profile | balanced | not supported | daemon `power.set_profile` |
| actions.controls.profile.performance | power-profile | performance | not supported | daemon `power.set_profile` |
| actions.media.play-pause | media | playPause | not supported | daemon `player.*` (active player) |
| actions.media.next | media | next | not supported | daemon `player.*` (active player) |
| actions.media.previous | media | previous | not supported | daemon `player.*` (active player) |
| actions.find-files | scope | :  | not supported | search prefix |
| actions.saved.add-action | form | action | not supported | Settings `windows-workspaces/workflows` |
| actions.saved.add-snippet | form | snippet | not supported | Settings `windows-workspaces/workflows` |
| actions.saved.add-search | form | search | not supported | Settings `windows-workspaces/workflows` |
| actions.saved.add-bookmark | form | bookmark | not supported | Settings `windows-workspaces/workflows` |
| actions.saved.add-workspace | form | workspace | not supported | Settings `windows-workspaces/workflows` |
| actions.saved.add-roots | form | roots | not supported | Settings `windows-workspaces/workflows` |
| actions.dictation.toggle | command |  | works | argv |
| actions.dictation.setup | command |  | works | argv |
| actions.share | command |  | works | argv |
| software.themes.import | form | software-theme-import | not supported | inline form (B0199) |
| software.themes.create | settings | colors | not supported | Settings `colors` |
| software.extensions.import | form | software-extension-import | not supported | inline form (B0199) |
| software.packages.review-policy | settings | system | not supported | Settings `system` |
| actions.bring-visitors | bring-visitors |  | not supported | helper `monitor-workspaces bring` |
| calculation | copy | 2 | works | wl-copy |
| actions.emoji.x | copy | 😀 | works | wl-copy |
| actions.clipboard.1 | copy-clip | clip-1 | works | daemon `clipboard.copy` |
| apps.audio.output-1 | audio-output | alsa_output.pci | not supported | daemon `audio.set_default` |
| style.wallpapers.abc | wallpaper | /home/u/Pictures/Wallpapers/a.png | not supported | daemon `wallpaper.activate` |
| style.accents.choice-0 | accent | #7a5af8 | not supported | helper `wallpaper-media theme` |
| actions.windows.1 | focus | 42 | not supported | daemon `window.focus` |
| actions.windows.2 | focus | 0x55d1 | not supported | daemon `window.focus` |
| setup.network.ap | settings | network | not supported | Settings `network` |
| setup.bluetooth.dev | settings | bluetooth | not supported | Settings `bluetooth` |
| learn.shortcuts.binding-0 | info | shortcut | not supported | stays selected |
| actions.reminders.unit | timer-cancel | nacre-reminder-123.timer | not supported | helper `menu-backend cancel-reminder` |
| software.webapps.x | webapp | x-site | not supported | `nacre-menu webapp-launch` |
| software.installed.x | remove-package | htop | not supported | kitty: `menu-backend remove` |
| software.packages.x | install-package | btop | not supported | kitty: `software-package install` |
| style.fonts.inter | font | Inter | not supported | desktop store `interfaceFont` |
| style.mono-fonts.jb | mono-font | JetBrains Mono | not supported | desktop store `monoFont` |
| software.development.node-install | environment | node | not supported | kitty: `software-environments run` |
| software.tui.htop | tui-launch | htop | not supported | helper `software-library` |
| software.tui.htop-remove | tui-remove | htop | not supported | helper `software-library` |
| software.ai.claude | ai-terminal | claude | not supported | kitty --hold |
| software.discover.site | webapp-offer | https://example.org | not supported | helper `menu-backend webapp-create` |
| software.themes.preset | software-theme | catppuccin | not supported | helper `wallpaper-media theme` |
| software.themes.bundle | software-theme | bundle-1 | not supported | helper `software-library bundle-apply` |
| software.extensions.x | software-extension-install | x-ext | not supported | helper `software-library install-builtin` |
| software.extensions.y | software-extension-open | y-ext | not supported | helper `software-library open` |
| software.extensions.theme | extension-theme | z-ext | not supported | helper `software-library` |
| system.speed.x | system-action | zram | not supported | helper `software-library system-open` |
| actions.palette.brain | legacy-action | notes | not supported | AI sidebar (`nacre left`) |
| nacre-action:colors: | legacy-action |  | not supported | Colors app |
| nacre-action:preset:focused | legacy-action | focused | not supported | daemon `settings.desktop preset` |
| nacre-action:toggle-setting:blur | legacy-action | blur | not supported | desktop store flip |
| nacre-action:dark: | legacy-action |  | not supported | daemon `appearance.set_mode` |
| nacre-action:overview: | legacy-action |  | not supported | overview panel |
| files.1 | open-file | /home/u/Documents/a.pdf | not supported | xdg-open |
| actions.assistant.search.n | note | /home/u/Documents/Nacre-Brain/wiki/a.md | not supported | xdg-open (no Brain explorer yet) |
| actions.search-web | search-web | rust lifetimes | not supported | `nacre-app browser` URL |
| actions.search-web-old | search-web | rust lifetimes | not supported | `nacre-app browser` URL |
| actions.assistant.question | ask | what is niri | not supported | daemon `ai.chat.new`+`send`, sidebar |
| actions.saved.engine | workflow | search-1 | not supported | helper `launcher-workflows activate` |
