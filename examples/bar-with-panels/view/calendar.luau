-- SPDX-License-Identifier: MIT
-- The calendar panel: opened by the clock button, closed by Escape, an outside click or Close.
local ui = require("bead.ui")
return ui.Column {
    id = "calendar", width = "fill", height = "fill", padding = 16, gap = 8, fill = "surface",
    ui.Text { text = ui.bind("desktop.date"), font_size = 22, height = 30, color = "primary" },
    ui.Text { text = ui.bind("desktop.clock.seconds"), font_size = 15, height = 22, color = "onSurface" },
    ui.Button { id = "close", text = "Close", width = 72, height = 28, padding = 4, font_size = 13, radius = 5,
        fill = "primary", color = "onPrimary", on_activate = ui.action("panel.close") },
}
