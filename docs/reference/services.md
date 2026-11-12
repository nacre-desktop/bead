# Folder app services

Services are opt-in through `[services].desktop` and `[commands.NAME]`. The host publishes only declared desktop service groups, and it updates only keys declared as stores in `behavior.luau`. Declare each key with the matching default type before binding it. `bead check` supplies an empty snapshot and does not start services or commands.

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

`desktop.date` is provided by the `clock` service. Clock seconds updates are leased when `clock` is declared.

## Actions

| Action | Argument | Requirement and effect |
|---|---|---|
| `desktop.volume` | Number | Requires `audio`; clamps finite values to 0–1. |
| `desktop.mute` | Bool | Requires `audio`; sets mute state. |
| `desktop.tray.activate` | item ID Text | Requires `tray`; sends pointer coordinates from the action event. |
| `desktop.tray.secondary` | item ID Text | Requires `tray`; sends pointer coordinates. |
| `desktop.tray.menu` | item ID Text | Requires `tray`; sends pointer coordinates. |
| `command.NAME` | Any/unused | Runs declared command `NAME`; unknown names produce no result. |

Wrong arguments produce explicit action errors: `desktop.volume expects a number`, `desktop.mute expects a boolean`, and tray actions require a text item id. Desktop actions against undeclared services report the missing service. Commands run without a shell; output, timeout, and interval limits are in the [manifest reference](manifest.md).

## Commands stores

For each declared command, declare these stores in `behavior.luau`:

```luau
stores = { ["commands.kernel.stdout"] = "", ["commands.kernel.ok"] = false,
    ["commands.kernel.error"] = "" }
```

`stdout` and `error` are Text, `ok` is Bool. Stored stdout is capped at 4 KiB. A periodic interval never overlaps an in-flight invocation. The runtime is shared by the folder host's views.
