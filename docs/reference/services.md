# Folder app services

Services are opt-in through `[services].desktop` and `[commands.NAME]`. The host publishes only declared desktop service groups. FolderView declares typed defaults for the keys published by each declared service and command, so `behavior.luau` can bind them directly. A view may repeat a key with the same type; conflicting types are a validation error. `bead check` supplies an empty snapshot and does not start services or commands.

## Desktop stores

All values are under `desktop.`. Missing numeric readings use `-1`; missing strings are empty and missing booleans are false.

| Key | Type | Service |
|---|---|---|
| `clock` | Text | `clock` |
| `clock.seconds` | Text | `clock` |
| `date` | Text (`YYYY-MM-DD`, or empty if invalid) | `clock` |
| `audio.available` | Bool | `audio` |
| `audio.volume` | Number (0–1, or -1 unavailable) | `audio` |
| `audio.muted` | Bool | `audio` |
| `audio.sink` | Text | `audio` |
| `battery.available` | Bool | `battery` |
| `battery.percent` | Number, -1 if unknown | `battery` |
| `battery.charging`, `battery.on_battery` | Bool | `battery` |
| `network.available`, `network.connected`, `network.wireless` | Bool | `network` |
| `network.name` | Text | `network` |
| `network.strength` | Number, -1 if unknown | `network` |
| `bluetooth.available`, `bluetooth.powered` | Bool | `bluetooth` |
| `bluetooth.connected` | List of `{name: Text}` records | `bluetooth` |
| `tray` | List of `{id,title,icon_name: Text, attention,has_menu: Bool}` records | `tray` |
| `session.locked` | Bool | `session` |
| `media.available` | Bool | `media` |
| `media.player`, `media.status`, `media.title`, `media.artist`, `media.album`, `media.art_url` | Text | `media` |
| `media.length_ms`, `media.position_ms` | Number, -1 if unavailable | `media` |
| `media.can_play_pause`, `media.can_next`, `media.can_previous`, `media.can_seek` | Bool | `media` |
| `workspaces` | List of `{id: Text, index: Number, name: Text, output: Text, active: Bool, focused: Bool, urgent: Bool}` | `workspaces` |
| `window.title`, `window.app_id` | Text; focused window, empty when none | `workspaces` |
| `compositor` | Text: `niri`, `hyprland`, or empty when undetected | `workspaces` |
| `power.available` | Bool | `power` |
| `power.profile`, `power.degraded` | Text | `power` |
| `power.profiles` | List of `{name: Text}` records, in daemon order | `power` |
| `network_devices.available`, `network_devices.wireless_enabled` | Bool | `network_devices` |
| `network_devices.primary_name`, `network_devices.primary_kind` (`wifi`, `ethernet`, `vpn`, `other`, or empty) | Text | `network_devices` |
| `network_devices.access_points` | List of `{ssid,security: Text, strength: Number, active,known: Bool}` records, one per SSID, strongest first | `network_devices` |
| `network_devices.known` | List of `{id,kind: Text}` saved connections | `network_devices` |

`desktop.date` is provided by the `clock` service. Clock seconds updates are leased when `clock` is declared.
The optional `media` service selects the active MPRIS player using the policy in [spec 0239](../specs/0239-mpris.md). While it is Playing, `media.position_ms` is extrapolated and refreshed at 1 Hz without additional D-Bus reads.
The optional `power` service follows power-profiles-daemon and publishes its active profile, available profiles, and performance degradation reason as described in [spec 0244](../specs/0244-power-profiles.md).
The optional `bluetooth_devices` service publishes BlueZ adapters and devices and provides power, scan, connect, disconnect, and forget actions as described in [spec 0246](../specs/0246-bluetooth.md). Declare it in the folder manifest, then pass a selected device path to the connect action:

```toml
[services]
desktop = ["bluetooth_devices"]
```

```luau
local devices = stores["desktop.bluetooth_devices.devices"]
-- An item action can send devices[1].path as its Text argument.
```

The optional `network_devices` service follows NetworkManager and is separate from the `network` summary; see [spec 0247](../specs/0247-network.md). `connect` takes an SSID from `access_points`: known networks use their saved profile, open networks are added without secrets, and other secured networks report `secured network needs a password agent`.

The optional `workspaces` service starts one compositor adapter shared by all views. Detection prefers a non-empty `$NIRI_SOCKET`, then `$HYPRLAND_INSTANCE_SIGNATURE` under `$XDG_RUNTIME_DIR/hypr`. Updates follow IPC events; reconnect uses bounded exponential backoff. Workspace `id` and `output` are opaque Text values (`output` is empty when unknown). Indices are labels, not identities: niri indices are per output, while Hyprland indices are global. Pass the record's `id` to target an exact workspace. No detected compositor means an empty list, empty window fields, and an empty compositor name; focus actions report a clear error. See [spec 0249](../specs/0249-compositor-adapters.md) and [hello-status](../../examples/hello-status/).

## Actions

| Action | Argument | Requirement and effect |
|---|---|---|
| `desktop.volume` | Number | Requires `audio`; clamps finite values to 0–1. |
| `desktop.mute` | Bool | Requires `audio`; sets mute state. |
| `desktop.mute.toggle` | None | Requires `audio`; sets mute to the inverse of the latest snapshot state. |
| `desktop.tray.activate` | item ID Text | Requires `tray`; sends pointer coordinates from the action event. |
| `desktop.tray.secondary` | item ID Text | Requires `tray`; sends pointer coordinates. |
| `desktop.tray.menu` | item ID Text | Requires `tray`; sends pointer coordinates. |
| `desktop.media.play_pause`, `desktop.media.next`, `desktop.media.previous` | None | Requires `media`; targets the active player. |
| `desktop.media.seek` | Number milliseconds | Requires `media`; targets the active track and fails if seeking is unavailable or the track changed. |
| `desktop.power.set` | Profile name Text | Requires `power`; accepts only a profile in the daemon's current list. |
| `desktop.power.cycle` | None | Requires `power`; selects the next profile in daemon order, wrapping at the end. |
| `desktop.network_devices.wireless` | Bool | Requires `network_devices`; enables or disables Wi-Fi. |
| `desktop.network_devices.scan`, `desktop.network_devices.disconnect` | None | Requires `network_devices`; scans on the first Wi-Fi device (at most every 10 seconds), or disconnects the first connected one. |
| `desktop.network_devices.connect` | SSID Text | Requires `network_devices`; activates a known or open network. |
| `desktop.network_devices.forget` | Saved connection id Text | Requires `network_devices`; deletes that saved connection; ambiguous ids are refused. |
| `desktop.workspace.focus` | Number integer index or opaque workspace ID Text | Requires `workspaces`; focuses a workspace. |
| `desktop.window.focus` | Opaque window ID Text | Requires `workspaces`; focuses a window. |
| `command.NAME` | Any/unused | Runs declared command `NAME`; unknown names produce no result. |
| `panel.open`, `panel.toggle` | panel name Text | Requires `[panels]`; opens (or toggles) that popup against the bar under the pointer. |
| `panel.close` | None | Requires `[panels]`; closes the open popup. |

Wrong arguments produce explicit action errors: `desktop.volume expects a number`, `desktop.mute expects a boolean`, and tray actions require a text item id. Desktop actions against undeclared services report the missing service. Commands run without a shell; output, timeout, and interval limits are in the [manifest reference](manifest.md).

FolderView declares the host actions provided by declared services and commands to view validation, so no behavior handler is needed for them. An undeclared `desktop.*` action reports the required `[services].desktop` declaration; an undeclared `command.NAME` action reports the missing `[commands.NAME]` entry. Other hosts keep behavior-script action validation unless they call `Session::set_host_actions`.

## Commands stores

Each declared command automatically provides these typed stores:

```luau
stores = { ["commands.kernel.stdout"] = "", ["commands.kernel.ok"] = false,
    ["commands.kernel.error"] = "" }
```

`stdout` and `error` are Text, `ok` is Bool. Stored stdout is capped at 4 KiB. A periodic interval never overlaps an in-flight invocation. The runtime is shared by the folder host's views.
