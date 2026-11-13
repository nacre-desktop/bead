-- SPDX-License-Identifier: MIT
-- The volume panel: audio actions route to the declared service shared with the bar.
local ui = require("bead.ui")
local kit = require("bead.kit")
local available = ui.resolve_for_check(ui.bind("desktop.audio.available"))
local volume = ui.resolve_for_check(ui.bind("desktop.audio.volume"))
return ui.Column {
    id = "volume-panel", width = "fill", height = "fill", padding = 16, gap = 10, fill = "surface",
    ui.Text { text = ui.bind("volume"), font_size = 16, height = 24, color = "onSurface" },
    kit.Slider { id = "level", width = 228, min = 0, max = 1, step = 0.05,
        value = volume, enabled = available, on_change = "desktop.volume", label = "Volume" },
    ui.Row {
        id = "controls", gap = 8,
        ui.Button { id = "mute", text = ui.bind("mute"), width = 72, height = 30, padding = 4, font_size = 13, radius = 5, fill = "primary", color = "onPrimary", on_activate = available and ui.action("desktop.mute.toggle") or nil },
        ui.Button { id = "close", text = "Close", width = 60, height = 30, padding = 4, font_size = 13, radius = 5, fill = "surface", color = "onSurfaceVariant", on_activate = ui.action("panel.close") },
    },
}
