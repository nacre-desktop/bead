-- SPDX-License-Identifier: MIT
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
