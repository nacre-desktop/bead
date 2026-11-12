-- SPDX-License-Identifier: MIT
local ui = require("bead.ui")
local children = {
    id = "hello-window", width = "fill", height = "fill", padding = 20, gap = 12,
    fill = "surface",
    ui.Text { text = "Hello from Bead", font_size = 22, height = 30, color = "primary" },
    ui.Text { text = "A small Luau app", font_size = 15, height = 24, color = "onSurface" },
    ui.TextInput { id = "name", text = ui.bind("name"), placeholder = "Type your name", height = 42,
        font_size = 16, padding = 8, fill = "surface", color = "onSurface", border_width = 1,
        border_color = "outlineVariant", radius = 6 },
    ui.Button { id = "counter-button", text = ui.bind("counter"), on_activate = ui.action("increment"), height = 42,
        font_size = 16, padding = 8, fill = "primary", color = "onPrimary", radius = 6 },
}
return ui.Column(children)
