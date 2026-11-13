# Folder app manifest

`bead` reads `bead.toml` from the app folder. Unknown keys are rejected at every table. `id` and `title` must be non-empty strings. Exactly one surface table, `[window]` or `[layer]`, is required. TOML parse errors include the manifest path and parser location.

| Key | Type, default, limits | Example |
|---|---|---|
| `id` | Required string; non-empty after trimming. Used as surface namespace/app identity. | `id = "org.example.Clock"` |
| `title` | Required string; non-empty after trimming. | `title = "Clock"` |
| `view` | Relative path string; default `view/main.luau`; must stay inside app folder. | `view = "ui/main.luau"` |
| `palette` | Relative path string; default `palette.json`; must stay inside app folder. | `palette = "theme/palette.json"` |
| `[window].size` | Required `[u32, u32]`; both dimensions > 0. | `size = [440, 300]` |
| `[window].min_size` | Optional `[u32, u32]`; both > 0; runtime default `[320, 240]`. | `min_size = [320, 240]` |
| `[layer].edge` | Required enum: `top`, `bottom`, `left`, `right`. | `edge = "top"` |
| `[layer].size` | Required positive `u32` thickness. Top/bottom checks use 1280×size; left/right use size×720. | `size = 36` |
| `[layer].exclusive` | Boolean; default `true` (reserve the edge). | `exclusive = false` |
| `[services].desktop` | Array of strings; default `[]`. Allowed: `clock`, `audio`, `battery`, `network`, `bluetooth`, `tray`, `session`, `media`, `power`, `bluetooth_devices`, `network_devices`, `workspaces` (the host's `DESKTOP_SERVICES`). | `desktop = ["clock", "audio"]` |
| `[commands.NAME].argv` | Required non-empty string array; no argument may contain NUL. Executed directly, without a shell. | `argv = ["uname", "-r"]` |
| `[commands.NAME].interval` | Optional positive integer seconds; no default periodic run. | `interval = 60` |
| `[commands.NAME].timeout` | Integer seconds, default `5`, range 1–60. | `timeout = 10` |

Command names are 1–32 ASCII characters, each lowercase letter, digit, `_`, or `-`. Command process output is capped at 64 KiB and stored stdout at 4 KiB. Interval executions do not overlap.

## Exact validation errors

The CLI reports these app validation messages (prefixed by `bead.toml`'s path):

- Empty identity: `id and title must not be empty`.
- Neither or both surface tables: `specify exactly one of [window] or [layer]`.
- Zero window dimension: `window sizes must be positive`.
- Zero layer thickness: `layer.size must be positive`.
- Unsupported desktop service: `unknown desktop service 'NAME'`.
- Invalid command name: `invalid command name 'NAME'`.
- Empty argv or embedded NUL: `command 'NAME' argv must be non-empty and contain no NUL`.
- Invalid timeout: `command 'NAME' timeout must be 1..=60 seconds`.
- Zero interval: `command 'NAME' interval must be at least 1 second`.
- Absolute or escaping view/palette path: `manifest path must be relative to the app folder` or `path escapes the app folder`.
- Unknown keys and wrong TOML types are rejected by TOML deserialization with a path and parser detail.

`services` and `commands` are optional. `bead check` uses empty service snapshots and starts neither services nor commands. See [services](services.md) for available values and actions.
