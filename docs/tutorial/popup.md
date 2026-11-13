# Add a popup to a bar

Continue from [A bar in 50 lines](bar.md): make the clock open a panel. A panel is another Luau view in the same folder, declared in `bead.toml`, that the host shows next to the bar when the view asks for it. The complete example is [`examples/bar-with-panels`](../../examples/bar-with-panels/).

## 1. Declare the panel

Add to `bead.toml` (panels need a `[layer]` bar):

```toml
[panels.calendar]
view = "view/calendar.luau"
size = [260, 120]
```

`size` is the panel's logical size. The view's sibling `behavior.luau` and `bundle.json` apply, so `view/` can hold the bar and its panels together.

## 2. Open it from the bar

Turn the clock label into a button:

```luau
ui.Button { id = "clock", text = ui.bind("desktop.clock"), width = 72, height = 28, padding = 4,
    on_activate = ui.action("panel.toggle", "calendar") },
```

`panel.toggle`, `panel.open` and `panel.close` are host actions available once `[panels]` is declared; no behavior handler is needed. The panel opens flush against the bar, centred under the pointer and kept inside the output, on the output where the click happened.

## 3. Write the panel view

Create `view/calendar.luau`:

```luau
local ui = require("bead.ui")
return ui.Column {
    id = "calendar", width = "fill", height = "fill", padding = 16, gap = 8, fill = "surface",
    ui.Text { text = ui.bind("desktop.date"), font_size = 22, height = 30, color = "primary" },
    ui.Button { id = "close", text = "Close", width = 72, height = 28, padding = 4, on_activate = ui.action("panel.close") },
}
```

The panel shares the bar's declared services and commands, so it binds `desktop.date` directly. Escape and a click outside also close it; one panel is open at a time, and opening another replaces it. Closing keeps the current panel prepared for reopening; selecting a different panel replaces that worker.

`bead check` validates and renders every panel view at its declared size, with the same empty service snapshot as the bar.
