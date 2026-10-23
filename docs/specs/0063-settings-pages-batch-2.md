# B0063 - Settings pages batch 2 (capabilities, date/time, history, workflows, devices, system pages)

Status: **In progress** (Claude worker, branch `task/b0063-settings-p2-20261023`). Builds on B0045 (framework) and B0058 (batch 1).
Nacre contracts used: N0049 (`57b3b1fe`: `time`, `capabilities`, `login.install_theme`, history/workflow routes), N0046
(audio, brightness, power), N0047 (Bluetooth), N0042 (Wi-Fi). Nacre reference for the QML pages:
`nacre/shell/modules/settings/{pages,prototype}/*` at `origin/main`. `nacre-protocol` pin unchanged (the adapter reads topics as JSON).

## Design (nothing here is Nacre layout in Rust)

- **Engine**: `visibleWhen` / `enabledWhen` are evaluated in the registry engine for pages, sections, settings, actions and search
  (`all`, `any`, `not`, `capability`). Flags come from the `capabilities` topic. A flag the topic does not name is false (QML); with
  no flags published at all everything is shown. `bluetooth` and `battery` are named by the registry but not carried by the topic:
  they are read from the `bluetooth` and `battery` topics (**Nacre ask C1**: add both flags).
- **Adapter** `nacre-shell/src/settings/pages.rs` (GPL, the Nacre edge): the custom sections (`customType`) and the registry
  `actions` become generic rows: `info`, `actions` (button choices), `device` (title, detail, buttons, optional slider), `picker`
  (searchable list), `secret` (masked field), `help`. It knows topics and methods, never layout. One daemon call at a time; level
  changes queue the newest value; every id in an argument is re-resolved against the published topic (never trusted).
- **Luau** `shell/settings/main.luau` maps each row kind to a kit component.
- **New kit components** (general, in `bead-ui/assets/kit.luau`): `InfoRow` (label, value, description), `ChipsRow` (label over wrapping
  option chips), `DeviceRow` (icon, title, detail, buttons, optional slider line), `PickerList` (filter field + entries + count of
  the rest; the zone picker and, later, an app picker), `SecretRow` (masked field with join and cancel), per-option `enabled` in
  `ChoiceRow`. To list under ADR 0004 "Generalization": B0063 above.

## Pages

| Page | Bead | Nacre ask / gap |
| --- | --- | --- |
| date-time-language | time zone (picker with search, `time.zones`, `time.set_timezone`), network time (`time.set_ntp`), 12/24 h (`desktop.clockFormat`), language, units, weather | polkit refusal or no agent: `permission_denied` is shown as "Authentication was refused, or no authentication agent is running." The bar and dashboard clocks do not read `clockFormat` yet (Bead-side follow-up). "Automatic timezone" stays out (N0049) |
| notifications | history from the `notifications` window (20 newest, age, app, body), clear with a confirming press (`notifications.clear {scope:"history"}`) | no paging: "Older notifications are not shown" |
| windows-workspaces/workflows | presets and workflow roles (`settings.desktop` `preset` / `save-workflow`) | the roles list is the controller's fixed set |
| lock-login | the four lock options, Lock now (`session.request`), Install login theme (`login.install_theme`, only when advertised) | preview layout is not offered (Bead previews its own lock); idle policy is planned |
| privacy | clipboard picker, notification history clear (confirm), online-lookup links | **C2**: `clipboard.clear_history`, `launcher.clear_history` (QML adapters `clipboard-history`, `launcher-history`); the retention numbers are planned |
| keyboard-mouse | repeat delay/rate, pointer, acceleration, left-handed, natural scroll, touchpad, all behind `input.controls`/`touchpad` | **C3**: store `software-input` for layouts and "touchpad enabled" (adapters `keyboard-layout`, `touchpad-enabled`); **C4**: the shortcuts list (`ShortcutsContent`) needs a published bindings topic |
| power | profile choices (store `power`), behind `powerProfiles`; page hidden without a battery | **C5**: `power-refresh` / `power-fix` adapters (shown only when the flag is false) |
| system | AUR review policy (store `profile`, apply) | **C6**: maintenance status and actions (`Maintenance`: release, services, config errors, drift, sync, recovery); updates; Welcome; AUR provenance |
| nacre-ai | default assistant (store `ai-settings`), link to sidebar options | **C7**: host launch route for "Open sidebar", "Accounts, usage and projects", Brain, Welcome (the app has no channel to the shell) |
| brain, addons | requires-contract lines | **C7** and **C8**: install add-ons (QML `install-ai`, `install-brain`) |
| sound | outputs, microphones and application audio as device rows: default, mute, volume slider (`audio.set_volume`/`set_mute`, perceptual cube) | **C9**: `audio.set_default {node}` is reserved but not advertised, so Default stays dim; confirm that the `audio` item id equals the PipeWire `node.name` (used as the call's `node`); audio routing manager launch (**C7**) |
| network | Wi-Fi switch, network list with connect (password row for new secured networks, sent once), saved networks with forget (confirming press) | hidden SSID join, VPN, and the connection editor (**C7**) are not in this page; the control center has hidden SSID |
| bluetooth | per adapter power and scan, device list with connect/disconnect for paired devices | pairing (B0055 panel) and trust/untrust (**C10**: `bluetooth.set_trusted`); device address is not published |
| displays | not built (own later unit): the page stays read-only | |

"Requires" lines read "`<control>`: requires a Nacre contract that is not published yet" (a disabled row, never a dead button).

## Checks

Fake-daemon tests (`settings_pages_tests.rs`): capability conditions on pages/sections/settings/search/`enabledWhen`, absent and partial
flags; per page: the exact method and parameter shape of each published call, refusal and stale handling, ids not in the published topic
send nothing, unadvertised methods send nothing. 17 goldens `settings-*` (CPU renderer, test symbols font). Typing in search unchanged.

## Look-and-feel parity (item 5)

See "Parity" below (filled in with the nested comparison or the stated gap).

## Temporary duplicates

None added. `shell/settings/registry/*` stays the verbatim registry copy of B0045 (delete 7 days after the daemon publishes the registry).
