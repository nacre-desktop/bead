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
| `power.available` | Bool | `power` |
| `power.profile`, `power.degraded` | Text | `power` |
| `power.profiles` | List of `{name: Text}` records, in daemon order | `power` |

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
| `command.NAME` | Any/unused | Runs declared command `NAME`; unknown names produce no result. |

Wrong arguments produce explicit action errors: `desktop.volume expects a number`, `desktop.mute expects a boolean`, and tray actions require a text item id. Desktop actions against undeclared services report the missing service. Commands run without a shell; output, timeout, and interval limits are in the [manifest reference](manifest.md).

FolderView declares the host actions provided by declared services and commands to view validation, so no behavior handler is needed for them. An undeclared `desktop.*` action reports the required `[services].desktop` declaration; an undeclared `command.NAME` action reports the missing `[commands.NAME]` entry. Other hosts keep behavior-script action validation unless they call `Session::set_host_actions`.

## Commands stores

Each declared command automatically provides these typed stores:

```luau
stores = { ["commands.kernel.stdout"] = "", ["commands.kernel.ok"] = false,
    ["commands.kernel.error"] = "" }
```

`stdout` and `error` are Text, `ok` is Bool. Stored stdout is capped at 4 KiB. A periodic interval never overlaps an in-flight invocation. The runtime is shared by the folder host's views.
