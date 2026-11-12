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

## 2. Declare typed state and display values

Create `view/behavior.luau`:

```luau
return {
    stores = {
        ["desktop.clock"] = "",
        ["desktop.battery.available"] = false,
        ["desktop.battery.percent"] = -1,
        ["desktop.audio.available"] = false,
        ["desktop.audio.volume"] = -1,
        ["desktop.audio.muted"] = false,
    },
    actions = { ["desktop.mute"] = function(_, _) end },
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

Every host value the view reads, including `desktop.*` and `commands.*`, must be declared as a store with a default of the matching type. The folder host updates only declared stores. This schema makes values typed and gives the host stable-ID state to preserve across view reloads.

Use `computed` for display strings and bind those names in the view. A computed function receives the current stores, so its result is refreshed when its inputs change. `ui.resolve_for_check(...)` only resolves a value while checking; text assembled from it will not update as runtime state changes.

The empty `desktop.mute` behavior action is a validation shim: current UI action validation expects every action name in `actions`, while the folder host intercepts `desktop.mute` and sends it to the audio service. It does not run this no-op handler. This framework seam should eventually recognize host actions during validation.

## 3. Lay out and bind the view

Create `view/main.luau`:

```luau
local ui = require("bead.ui")
return ui.Row {
    id = "bar", width = "fill", height = "fill", align = "center", gap = 12, padding = 12,
    fill = "surface",
    ui.Row { id = "left", width = 0, grow = 1, align = "center", ui.Text { text = "Tutorial", width = 90, height = 24, color = "primary" } },
    ui.Text { id = "clock", text = ui.bind("desktop.clock"), width = 64, height = 24, text_align = "center", color = "onSurface" },
    ui.Row { id = "right", width = 0, grow = 1, justify = "end", align = "center", gap = 8,
        ui.Text { id = "battery", text = ui.bind("battery"), width = 130, height = 24, color = "onSurfaceVariant" },
        ui.Button { id = "mute", text = ui.bind("mute"), width = 72, height = 30, color = "onPrimary", fill = "primary", on_activate = ui.resolve_for_check(ui.bind("desktop.audio.available")) and ui.action("desktop.mute", true) or nil },
        ui.Text { id = "volume", text = ui.bind("volume"), width = 120, height = 24, color = "onSurfaceVariant" },
    },
}
```

The left and right sections each grow into half of the remaining space, which keeps the fixed-width clock centered. Every `Text` has a width, so the first child cannot consume the row; `align = "center"` centers each section's children vertically. `ui.bind` subscribes displayed values to store updates. The mute button sends `desktop.mute` with `true`, so clicking it mutes audio. The availability guard lets offline `bead check` skip validating the service action. `ui.resolve_for_check(...)` is for check-time inspection only; do not use it to construct runtime text or values that need to follow live state.

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

- Browse the [example gallery](../../examples/README.md).
- Look up elements and state in the [Luau UI reference](../reference/luau-ui.md).
- Add a declared command using the [manifest](../reference/manifest.md) and [services guide](../reference/services.md).
- Use [bead.kit](../reference/kit.md) for reusable controls, or write a Rust host using the [host reference](../reference/rust-hosts.md).
