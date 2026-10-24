# B0084 - every Nacre route, keybind and entry point with Bead as the shell

Status: **Review** (Claude worker, branches `task/b0084-qml-routes-20261024` here and `task/b0084-routes-20261024` in Nacre).
Last unit before the owner's trial switch (`nacre ui-shell bead`, Nacre N0054). Method: read every Bead handler
(`ui_provider.rs`, `key_routes.rs`, `commands.rs::map_views`, `reload.rs`, B0026/B0068/B0077) and every Nacre entry point
(`shell-routes.json`, `niri/bindings.kdl`, `hypr/conf/*.lua`, `nacre/shell-tools/control.sh`, `compositor-contract.json`, the helper
sources that still call `qs ipc`), then run each shipped call through Bead's own catalog and mapper (new tests below).
Nacre baseline `c6dc627b`.

## Result in one paragraph

Bead already answers every `nacrectl ui` route; the table in `shell-routes.json` was stale (it still said "gap" for rows B0068 did).
Of 21 routes 15 work, 3 work with a stated refusal of QML-only preview state, 2 are acknowledged no-ops by design and 1
(`menuPrompt open`, the selector prompt) is **not implemented and fails fast**. Real defects found and fixed: the lock-screen media
buttons only worked through `qs ipc` (broken under Bead) and the Settings/Colors/Welcome window rules matched only the QML
`org.quickshell` windows (Bead's apps carry their own app ids, so they would have tiled instead of floating and centering).
Brightness readback and notification history already work through the daemon. Two things stay open and are written as asks:
the selector prompt, and the lock screen's prepared content (`lock-prepare`), which only QML feeds.

## Inventory

Verdicts: **works** / **works, limit** (a stated refusal of a QML-only preview flag) / **no-op** (acknowledged by design) /
**broken** (needs a Bead target) / **obsolete** (QML-internal, not reachable once Bead is the shell) / **fixed** (broken, fixed in this unit).

### Routes (`nacre/shell-tools/shell-routes.json`, 21)

| Route | Bead handler | Verdict |
| --- | --- | --- |
| `nacre launcher [Q]` | `key_routes::launcher` (toggle) | works |
| `nacre menu ROUTE Q`, `nacre clipboard` | `key_routes::launcher` | works |
| `nacre mode apps\|wallpaper\|clipboard` | launcher, menu routes | works |
| `nacre mode overview\|palette\|keys` | independent extras surfaces (B0077) | works |
| `nacre mode legacy` | refused ("legacy launcher mode is not implemented") | works, limit (nothing in Nacre issues it) |
| `nacre hide`, `nacre close` | `bead.panel hide` / `close` | works |
| `nacre settings [PAGE]` | Settings app (`app::route`) | works |
| `nacre left`, `drawers toggle left` | chat app | works |
| `nacre controls home\|edit\|network\|bluetooth\|battery\|sound\|audio\|calendar` | control centre / panels | works |
| `nacre controls session\|notifications` | session view / Notices page | works |
| `nacre controls updates` | refused (no updates view) | works, limit (Nacre's updater is a launcher/Settings flow) |
| `nacre session`, `drawers toggle session` | `bead.panel toggle session` | works |
| `drawers toggle dashboard\|right\|sound\|launcher` | dashboard / control / sound / launcher | works |
| `drawers toggle osd\|leftPinned\|dashboardPinned\|previewOnly` | refused | works, limit (QML preview/pin flags; the OSD is event driven) |
| `brightness step up\|down` | daemon `brightness.adjust` on the focused output | works |
| `brightness set X` | daemon `brightness.set` (fraction 0..1 -> percent) | works |
| `keyboardLight step up\|down` | daemon `keyboard_light.adjust` | works |
| `pearls detach`, `pearls open launcher\|ai\|dashboard\|media\|controls` | panel layer | works |
| `colorsApp open [IMAGE]`, `welcomeApp open PAGE` | on-demand apps | works |
| `welcomeApp refresh` | acknowledged | no-op (Bead's Welcome keeps no first-run state) |
| `menuPrompt cancel ID` | acknowledged | no-op (nothing is ever open) |
| `menuPrompt open ID` | answers the waiting `nacre-menu pick\|input` with `{cancelled:true}` through `nacre-helper menu-cli respond` | **broken** (fails fast; see Asks 1) |

Counts: 15 works, 3 works-with-limit, 2 no-op, 1 broken, 0 left as "gap". `nacre.mode` and `nacre.controls` and `drawers.toggle` are
the three limited rows.

### `nacre` CLI commands (`control.sh`, 49)

| Group | Commands | Verdict |
| --- | --- | --- |
| `ui` (18) | launcher, menu, keys, clipboard, palette, overview, hide, close, settings, left, controls, session, toggle, brightness, keyboard-light, detach, pearl, wallpaper (no args) | works (routes above; every one ends in `nacrectl ui`, none in `quickshell ipc`) |
| native, shell independent (27) | dolphin, thunar, scheme-mode, wallpaper-image, doctor, profile, check, rollback, updates, bluetooth, tasks, new, resume, lock, files, terminal, network, audio, volume, focus, workspace, play, next, previous (playerctl), mute, brain, capture | works (no shell involvement; `play/next/previous` use `playerctl`, `lock` uses `loginctl`) |
| native, uses the selector prompt (2) | `wifi`, `apps` with several windows of one app (`desktop-actions pick`) | **broken** only for the choice step, same cause as `menuPrompt open` (Asks 1) |
| `start` | runs the QML supervisor | obsolete under Bead: the N0054 drop-in replaces the unit's ExecStart; running `nacre start` by hand while Bead is the shell would start a second (QML) shell. Do not run it during the trial. |
| `lock-preview` | `qs -p lock-preview.qml`, a standalone QML window | works while `quickshell` is installed (independent of the shell service); Bead's Settings hides the row. Retires with QML. |

### Nacre apps / wrappers (`apps` table)

`nacre-settings [PAGE]` -> `nacrectl ui nacre settings`; `nacre-welcome` (+`--login`, `--enable|--disable`) -> `welcomeApp open|refresh`;
`nacre-colors` -> `colorsApp open`; `nacre-menu ROUTE` -> `nacre menu`; `nacre-menu pick|input` -> `menuPrompt open` (broken, Asks 1);
`nacre-helper desktop-actions brightness N` -> `brightness set`; `~/.local/bin/nacre-shell detach|brightness|session|...` -> `nacrectl ui`.
All **works** except the prompt. Window rules: see "Fixed".

### Compositor binds and autostart

All shell binds are `nacrectl ui` calls (niri `bindings.kdl` and Hyprland `keybinding.lua`/`shortcuts.lua` are generated from one
contract, so the two backends agree). `shell/keybind-calls.json` lists the 22 distinct calls and
`every_shipped_keybind_and_wrapper_call_is_accepted_by_the_provider` runs each through Bead's catalog check and mapper.

| Bind | Call | Verdict |
| --- | --- | --- |
| XF86MonBrightness Up/Down | `brightness step up\|down` | works |
| XF86KbdBrightness Up/Down | `keyboardLight step up\|down` | works |
| Super+Space, Super+W | `nacre launcher ""`, `nacre launcher ">wallpaper "` | works |
| Super+V | `nacre clipboard` | works |
| Super+. , Super+Ctrl+E | `nacre menu actions.emoji ""` | works |
| Super+Ctrl+C | `nacre menu actions.capture ""` | works |
| Super+X, Super+Esc | `nacre menu power ""` | works |
| Super+K, Super+F1 | `nacre menu learn.shortcuts ""` | works |
| Super+Ctrl+Space | `nacre mode palette` | works (B0077 surface) |
| Super+Tab (Hyprland) | `nacre mode overview` | works; niri uses `niri msg action toggle-overview` (no shell) |
| Super+Z | `nacre hide` | works |
| Super+O | `drawers toggle dashboard` | works |
| Super+A | `nacre left` | works |
| Super+I | `~/.local/bin/nacre-settings` -> `nacre settings` | works |
| Super+Ctrl+D | `nacre-shell detach` -> `pearls detach` | works |
| Super+B / Super+N | `nacre-brain open\|capture` | works (independent of the shell) |
| Print, Super+Shift+S, Super+Print | `grim`/`slurp`/`niri msg` + `notify-send` | works; the confirmation toast needs a notification owner (the trial's `--notifications none` has none, so it is silent) |
| volume, mute, mic, lock (Super+L, power key), window and workspace binds | `wpctl`, `loginctl`, `nacre-helper compositor action` | works (no shell) |
| Hyprland `Escape` global shortcut `nacre_shell:dismissHoverEdges` | answered by the QML `Shortcut` | obsolete until Bead has frame lips (B0047); Escape still closes Bead panels itself |
| Hyprland autostart (`autostart.lua`) | pam/polkit, `wl-paste --watch nacre-desktop-extras store`, `startup-apps.sh`, `nacre-welcome --login` | works (no `qs`; the shell is a systemd unit) |
| niri generated `shell.shortcut` (`qs ipc call nacreCompositor shortcut`) | the shipped contract has no such binding | obsolete (documented fallback; expiry Group F) |

### Internal QML consumers (the "fallbacks" rows, audited one by one)

| Consumer | Calls | With Bead as the shell |
| --- | --- | --- |
| `nacrectl ui` `legacy_ui` | `quickshell ipc` only when the daemon is absent or no provider is registered | obsolete: never reached once Bead registers; if it were, it fails with quickshell's "no instance" error |
| `daemon/session_policy.rs` | `ui.dispatch nacre state`, then `qs` only if no provider | works: Bead answers `nacre state` (with `launcherMode`) |
| `session_watch.rs` CLI | `qs ipc nacre state` | obsolete (the daemon observer replaces the CLI) |
| `shell_supervisor.rs` `health` | `qs ipc` | obsolete (only the QML supervisor, which the drop-in replaces) |
| `display_recover.rs` / `display-recover.py` | `qs ipc nacre state/restoreViews` | obsolete: only the QML `DisplayRecovery` queues it |
| `lock_info.rs` `lockWidgets state` | `qs ipc` | obsolete for the lock config (it calls `lock-info` only for the media buttons); now provider-first anyway |
| `lock_info.rs` media buttons (`mpris previous\|next\|playPause`) | `qs ipc` | **fixed** (below) |
| `lock-prepare` | fed by QML's `LockWidgets` presentation (colours, poster, media, weather, notifications) | **broken for freshness**: the lock still locks and shows the last prepared content, but nothing refreshes it under Bead (Asks 2) |
| `niri_hover_probe.rs`, `install.py validate_live` | `qs` | obsolete (installer/QML tooling; the trial runs no installer) |

## Fixed

| Where | Defect | Fix | Test |
| --- | --- | --- | --- |
| Nacre `lock_info.rs`, `daemon/client.rs` | `lock-info previous\|next\|toggle` (the hyprlock media buttons) ran `qs ipc ... mpris`: dead under Bead | `Info::media_key`: `ui.dispatch mpris ACTION` first (QML hosts it); if the provider refuses (Bead has no `mpris` target) the daemon answers: `player.play_pause\|next\|previous` on the active `dashboard.player` entity (`client::player_key`). No provider: the old `qs` path. `Info::ipc` is provider-first too. Nothing Bead-specific; nothing hardcoded. | `crates/nacre-helper/tests/lock_media_keys.rs` (fake daemon: QML-like provider gets exactly one `ui.dispatch`; refusing provider -> `sub dashboard.player` -> `player.*` with the active id) |
| Nacre `compositor-contract.json` (+ regenerated `niri/window-rules.kdl`, `hypr/conf/nacre.lua`) | Settings/Colors/Welcome rules matched `org.quickshell` + title; Bead's apps are `nacre-settings`, `nacre-colors`, `nacre-welcome` | three extra window rules (`centered`, `floating`) for those app ids; the QML rules stay for rollback | generator output committed; `tools/check.py` generated-file freshness, `test_shell_routes.py` |
| Nacre `shell-routes.json` | Bead statuses were stale ("gap" for rows B0068 did) | statuses and notes re-stated from the verified handlers; fallbacks rewritten with the verdicts above | `test_shell_routes.py`, `tests/ui_routes.rs` |
| Bead `shell/shell-routes.json` | snapshot of the file above | re-copied | `every_nacre_route_has_a_bead_handler_or_a_stated_refusal` |
| Bead | no guard that the calls Nacre's binds issue are accepted | `shell/keybind-calls.json` + `every_shipped_keybind_and_wrapper_call_is_accepted_by_the_provider` | itself |

No Bead product code changed: the audit found no route Bead refuses wrongly.

## Brightness readback: already works, no daemon ask

* **OSD** (key press, scroll, `brightness set`): the daemon's `brightness.adjust|set` reply is `{output, level_percent}`;
  `daemon/osd.rs::brightness` publishes it as an `osd` event (`kind: brightness`, `value` 0..1, `device`), which Bead's notice service
  shows (`notice_service.rs`, tests `notice_service_tests.rs` lines 320, 955, 993-1042). The value is the level after the change.
* **Slider / tile**: the control centre reads the `controls` topic (N0046): `brightness` per output (percent, from `light_devices::discover`,
  DDC for externals) and `keyboard.{level,max}`; `control_center.rs` `brightness.level` picks the panel (eDP/LVDS first) and shows the
  current percent, writes `brightness.set` (tests `control_center_tests.rs` 144-145, 714-765). The daemon re-reads after every change
  (`control_center::changed()`) and on its tick, so external changes (`brightnessctl`) show up too.
* Device discovery is the daemon's (`probe.outputs` x backlight/DDC); Bead needs none. A member is present only while its backend is.

## Notification history: works through the daemon

QML's history lived in `state/shell/notifications/history.json` via the `notification-history` helper; the daemon's `Store` opens the
**same file** with the same 30-day policy (`daemon/notifications.rs`, `notification_history.rs`). Bead's Notices page (B0020/B0024/B0030)
renders the daemon's `notifications` topic (`history` window, `history_count`, `history_more`, `history_storage`), so history survives a
Bead restart, a daemon restart (fixture `notifications-loss-restart.jsonl`) and a QML -> Bead -> QML round trip. Condition: the daemon must
own the bus (`nacre ui-shell bead --notifications daemon`). With the trial's `--notifications none` there is no notification owner and
therefore no new history until the owner switches the choice.

## Asks (nothing was blocking the switch)

1. **Selector prompt** (`menuPrompt open`): Bead needs a launcher pick/input mode that renders the request file's `choices`, answers with
   `nacre-helper menu-cli respond ID` and honours `expires`. Affects `nacre-menu pick|input`, the Wi-Fi chooser and the
   several-windows chooser of `desktop-actions`. Until then they cancel at once with a visible error instead of hanging. Own unit.
2. **Lock-screen content**: `lock-prepare` is fed by QML only. Under Bead the lock screen is secure and works, but its weather, media,
   notifications, colours and wallpaper stay as last prepared. Smallest shell-neutral fix: a daemon behaviour that builds the presentation
   from `wallpaper.active`, `dashboard.player`, `dashboard.weather`, `notifications` and `settings.store.desktop` and runs `lock-prepare`
   on change and before `lock-session` (Nacre unit; ~250 lines, so not done here). A Bead-side feeder would add always-on subscriptions to
   the always-on shell, against the memory budget.
3. **Hyprland Escape** (`dismissHoverEdges`): wait for B0047's frame lips.

## Temporary duplicates

None added. Existing, with their expiry: the QML window rules beside the Bead ones (delete 7 days after QML is retired); `legacy_ui`,
`shell.shortcut` and the QML probes listed above (Group F QML deletion pass); the `qs` fallback in `lock_info` (same pass).

## Checks

Both CI jobs ran on dev-2 in containers named `b0084-ci` (Arch, the Nacre `desktop` job) and `b0084-bead-ci` (ubuntu:24.04 with rust 1.99.0,
the Bead `check` job), 8 CPUs / 12 GiB each, and were removed afterwards.

* **Bead** (`task/b0084-qml-routes-20261024`): `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`,
  service-boundary check, `cargo test --workspace --locked` (747 passed, 0 failed, including
  `every_shipped_keybind_and_wrapper_call_is_accepted_by_the_provider` and `every_nacre_route_has_a_bead_handler_or_a_stated_refusal`),
  the `trace` memory-policy tests, renderer feature configurations, facade / headless UI / Luau schema, spikes tests and `py_compile`,
  packaging tests, motion-wallpaper smoke, private services lifecycle, SNI/DBusMenu lifecycle, `cargo deny` (workspace and framework
  crates): `FULL_CI_PASS`. Note: the workflow list in this tree has no `fake_system` suite (it arrives with B0083).
* **Nacre** (`task/b0084-routes-20261024`, pushed as `ab61b9f6` and `3d9825eb`, both `[no-publish]`, on `bbeb880f`): `cargo fmt --check`, `cargo clippy
  --all-targets -D warnings`, `cargo build --examples`, `cargo test` (1044 passed, 0 failed, including `lock_media_keys` and `ui_routes`),
  protocol minimal consumer, release helper build, helper sandbox integrity (29), translation coverage, `tools/check.py` (487 shell-tool
  tests, 534 helper parity cases, generated compositor config, window-class/floating consistency, QML/Lua/JS checks), pseudo-locale gallery
  audits: all green. The generated `niri/window-rules.kdl` and `hypr/conf/nacre.lua` were produced by `tools/generate_compositor_config.py`.
* Not run (needs the owner's session): hardware keys, the real lock screen, window placement of the three apps in the live compositor.
  The window rules are checked by generator output and the window-rule consistency check only. Visual changes: none, so no QML/Bead pairs.

Product lines: Bead 0 changed (tests and fixture only: +33 test lines, +30 fixture lines); Nacre +~120 helper lines (`client.rs` +84,
`lock_info.rs` +24) and +~110 test lines, +33 contract lines (+18 generated).
