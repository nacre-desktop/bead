# A bar in 50 lines

In about 15 minutes, make a top-edge bar with a title, live clock, battery reading, and a mute button. You need the `bead` CLI and a Wayland session to run it. The app itself is three files and fewer than 50 lines; palette and bundle files are host boilerplate and are not counted.

Create `examples/tutorial-bar/` with this layout:

```text
bead.toml
palette.json
view/behavior.luau
view/main.luau
view/bundle.json
```

`palette.json` and `view/bundle.json` are generated or copied boilerplate. Copy them from `examples/hello-bar/`; the app's behavior, layout, and services are in the three files below.

## 1. Declare the bar and its services

Create `bead.toml`:

```toml
id = "org.example.TutorialBar"
title = "Tutorial bar"

[layer]
edge = "top"
size = 36

[services]
desktop = ["clock", "battery", "audio"]
```

The `layer` table asks the host for a 36-pixel top-edge bar. `exclusive` defaults to `true`, so the bar reserves that strip. The services list opts into clock, battery, and audio snapshots; services are not started unless declared. The manifest reference explains all keys and limits.

## 2. Derive display values

Create `view/behavior.luau`:

```luau
return {
    stores = {},
    actions = {},
    computed = {
        battery = function(s)
            return s["desktop.battery.available"] and ("Battery " .. tostring(s["desktop.battery.percent"]) .. "%") or "Battery unavailable"
        end,
        volume = function(s)
            return s["desktop.audio.available"] and ("Volume " .. tostring(s["desktop.audio.volume"])) or "Audio unavailable"
        end,
        mute = function(s) return s["desktop.audio.muted"] and "Unmute" or "Mute" end,
    },
}
```

The declared `clock`, `battery`, and `audio` services provide their typed `desktop.*` stores automatically. Declared commands similarly provide `commands.NAME.stdout`, `.ok`, and `.error`. Add a store only for values your app owns or computes as mutable state.

Use `computed` for display strings and bind those names in the view. A computed function receives the current stores, so its result is refreshed when its inputs change. `ui.resolve_for_check(...)` only resolves a value while checking; text assembled from it will not update as runtime state changes.

FolderView recognizes actions provided by declared services and commands during UI validation, so host actions do not need behavior handlers.

## 3. Lay out and bind the view

Create `view/main.luau`:

```luau
local ui = require("bead.ui")
return ui.Row {
    id = "bar", width = "fill", height = "fill", align = "center", gap = 12, padding = 12,
    fill = "surface",
    ui.Row { id = "left", width = 0, grow = 1, align = "center", ui.Text { text = "Tutorial", width = "content", height = 24, color = "primary" } },
    ui.Text { id = "clock", text = ui.bind("desktop.clock"), width = "content", height = 24, text_align = "center", color = "onSurface" },
    ui.Row { id = "right", width = 0, grow = 1, justify = "end", align = "center", gap = 8,
        ui.Text { id = "battery", text = ui.bind("battery"), width = "content", height = 24, color = "onSurfaceVariant" },
        ui.Button { id = "mute", text = ui.bind("mute"), width = 72, height = 30, padding = 4, font_size = 14, color = "onPrimary", fill = "primary", on_activate = ui.action("desktop.mute.toggle") },
        ui.Text { id = "volume", text = ui.bind("volume"), width = "content", height = 24, color = "onSurfaceVariant" },
    },
}
```

The left and right sections each grow into half of the remaining space, which keeps the clock centered. `width = "content"` measures each label and includes its horizontal padding, so siblings keep their space; `align = "center"` centers each section's children vertically. `ui.bind` subscribes displayed values to store updates. The mute button sends `desktop.mute.toggle`, so each click flips the latest mute state. It remains wired when audio is unavailable; the host reports a diagnostic and does nothing. The kit Button has no bound disabled state.

## Run it

From the repository root:

```sh
bead check examples/tutorial-bar
bead run examples/tutorial-bar
```

`check` validates and CPU-renders without starting desktop services. `run` starts the declared services and opens the layer surface in your Wayland session.

## Reload on save

While running, edits trigger a reload. Bead validates a complete new view generation before swapping it in. A broken edit keeps the last working view active and reports a source location and error; fix the file and save again.

## Next steps

- [Add a popup](popup.md): a calendar panel under the clock.
- Browse the [example gallery](../../examples/README.md).
- Look up elements and state in the [Luau UI reference](../reference/luau-ui.md).
- Add a declared command using the [manifest](../reference/manifest.md) and [services guide](../reference/services.md).
- Use [bead.kit](../reference/kit.md) for reusable controls, or write a Rust host using the [host reference](../reference/rust-hosts.md).
