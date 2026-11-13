-- SPDX-License-Identifier: MIT
-- The bar: each button toggles a named panel from bead.toml under the pointer.
local ui = require("bead.ui")
return ui.Row {
    id = "bar", width = "fill", height = "fill", align = "center", gap = 8, padding_left = 12, padding_right = 12,
    fill = "surface",
    ui.Text { text = "Bar with panels", font_size = 15, color = "primary", width = 0, grow = 1 },
    ui.Button { id = "clock", text = ui.bind("desktop.clock"), width = 72, height = 28, padding = 4, font_size = 14, radius = 5,
        fill = "surface", color = "onSurface", on_activate = ui.action("panel.toggle", "calendar") },
    ui.Button { id = "volume", text = ui.bind("volume"), width = 130, height = 28, padding = 4, font_size = 14, radius = 5,
        fill = "surface", color = "onSurfaceVariant", on_activate = ui.action("panel.toggle", "volume") },
    ui.Button { id = "status", text = "Status", width = 72, height = 28, padding = 4, font_size = 14, radius = 5,
        fill = "surface", color = "onSurfaceVariant", on_activate = ui.action("panel.toggle", "status") },
}
