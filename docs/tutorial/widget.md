# Build your first shell widget

Add a plugin widget to the [tutorial bar](bar.md). The widget lives in its own folder, shows a greeting picked by a setting, and the bar mounts it without knowing its name. You need the `bead` CLI and a Wayland session to run it.

Work in the `my-bar/` folder from the bar tutorial. If you skipped it, copy the finished bar from the repository root:

```sh
cp -r examples/tutorial-bar my-bar
```

A plugin is a folder with a `plugin.toml` and Luau files. A folder app loads plugins from `<app>/plugins/<id>/` when its `view/bundle.json` says `"plugins": true`, as the tutorial bar's does. Plugins inside the app folder are bundled with it, so they are enabled and use their setting defaults.

## 1. Write the manifest

Create `my-bar/plugins/greeting/plugin.toml`:

```toml
id = "greeting"
name = "Greeting"
version = "0.1.0"
api = 1
capabilities = []

[[widget]]
id = "greeting"
entry = "widget.luau"

[settings.greeting]
type = "choice"
label = "Greeting"
options = ["Hello", "Hi", "Hey"]
default = "Hello"
```

The folder name must equal `id`: lowercase letters, digits and hyphens, starting with a letter. Unknown keys are rejected. Setting types are `toggle`, `choice`, `number`, `slider` and `text`. `capabilities = []` lets the widget draw, but not read the app's stores or send actions.

## 2. Write the widget

Create `my-bar/plugins/greeting/widget.luau`:

```luau
return function(api)
    local ui = api.ui
    return {
        render = function()
            return ui.Text { id = "greeting", text = tostring(api.plugin.settings.greeting), width = "content", height = 24, color = "onSurface" }
        end,
    }
end
```

The entry returns a factory that the host calls with an `api` table: here `api.ui` is `bead.ui` and `api.plugin` holds the plugin's id and settings. The host also chooses the shape of the returned table; this bar expects `render`. A plugin may require only `bead.ui`, `bead.kit` and its own files.

## 3. Mount widgets in the bar

Bead generates `plugins/index.luau` for the view. It lists every plugin with its `id`, `loaded`, setting `values` and `widgets`, each with a `module` path. Replace `my-bar/view/main.luau` with:

```luau
local ui = require("bead.ui")

local right = { id = "right", width = 0, grow = 1, justify = "end", align = "center", gap = 8,
    ui.Text { id = "battery", text = ui.bind("battery"), width = "content", height = 24, color = "onSurfaceVariant" },
    ui.Button { id = "mute", text = ui.bind("mute"), width = 72, height = 30, padding = 4, font_size = 14, color = "onPrimary", fill = "primary", on_activate = ui.action("desktop.mute.toggle") },
    ui.Text { id = "volume", text = ui.bind("volume"), width = "content", height = 24, color = "onSurfaceVariant" },
}

-- Append every loaded plugin widget to the right-hand section.
local index = require("../plugins/index")
for _, entry in index.plugins do
    if entry.loaded then
        for _, widget in entry.widgets do
            local plugin = require("../" .. widget.module)({ ui = ui, plugin = { id = entry.id, settings = entry.values } })
            table.insert(right, ui.plugin_gate(entry.id, plugin.render()))
        end
    end
end

return ui.Row {
    id = "bar", width = "fill", height = "fill", align = "center", gap = 12, padding = 12,
    fill = "surface",
    ui.Row { id = "left", width = 0, grow = 1, align = "center", ui.Text { text = "Tutorial", width = "content", height = 24, color = "primary" } },
    ui.Text { id = "clock", text = ui.bind("desktop.clock"), width = "content", height = 24, text_align = "center", color = "onSurface" },
    ui.Row(right),
}
```

`ui.plugin_gate(id, element)` checks every binding and action in the widget's output against the plugin's capabilities.

## 4. Check and run

```sh
bead check my-bar
bead run my-bar
```

"Hello" appears after the volume reading. A plugin with an invalid manifest is not mounted: its index entry has `loaded = false` and the reason in `error`, and the bar renders without it.

## 5. Change it live

While `bead run` is going, change `default` in `plugin.toml` to `"Hey"` and save: the bar reloads with the new greeting. A typo in `widget.luau` keeps the last working bar under a "Reload failed" notice, and the terminal prints the file and line, as for any view edit.

## Next steps

- Declare what the widget reads: with `capabilities = ["topic:desktop.clock"]` it may use `ui.bind("desktop.clock")`; without it, `bead check` fails with `plugin greeting lacks capability topic:desktop.clock`. `action:<prefix>` grants actions the same way.
- Limits: at most 64 plugins per host, 64 Luau files per plugin and 256 KiB per file. Luau sandboxing is a stability boundary, not a security boundary, so enable only plugins you trust.
- The full contract is [spec 0203](../specs/0203-plugins-v1.md).
