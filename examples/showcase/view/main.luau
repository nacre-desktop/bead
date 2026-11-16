-- SPDX-License-Identifier: MIT
-- The top bar: workspaces, a clock that opens the dashboard, now playing and status icons. Workspaces,
-- media, network, volume and battery are demo data (the battery is two boxes, not a glyph); the clock is the real desktop clock service.
local ui = require("bead.ui")
local kit = require("bead.kit")

local function workspace(n, focused)
    return kit.Button { id = "ws" .. n, text = tostring(n), width = focused and 40 or 26, height = 26, radius = 13, padding = 0,
        font_size = 13, text_align = "center", fill = focused and "primary" or "transparent",
        color = focused and "onPrimary" or "onSurfaceVariant", on_activate = ui.action("noop") }
end
local function status(id, glyph)
    return kit.IconButton { id = id, text = glyph, width = 28, height = 28, radius = 14, font_size = 18,
        fill = "transparent", color = "onSurface", on_activate = ui.action("noop") }
end

return ui.Row {
    id = "bar", width = "fill", height = "fill", align = "center", gap = 8, padding_left = 14, padding_right = 14,
    fill = ui.alpha("surface", .78),
    ui.Row { id = "left", grow = 1, width = 0, gap = 4, align = "center",
        workspace(1, false), workspace(2, true), workspace(3, false), workspace(4, false), workspace(5, false) },
    kit.Button { id = "clock", text = ui.bind("desktop.clock"), width = 96, height = 28, radius = 14, padding = 0,
        font_size = 15, text_align = "center", fill = ui.alpha("primaryContainer", .7), color = "onPrimaryContainer",
        on_activate = ui.action("panel.toggle", "dashboard") },
    ui.Row { id = "right", grow = 1, width = 0, gap = 4, align = "center", justify = "end",
        kit.Chip { id = "media", text = "Slow Light - Demo Artist", icon = "music_note", on = false, on_activate = ui.action("noop") },
        status("wifi", "wifi"), status("volume", "volume_up"), ui.Box { id = "battery", width = 26, height = 13, radius = 4, border_width = 1.5, border_color = "onSurface", padding = 2,
            ui.Box { id = "battery-level", width = "86%", height = "fill", radius = 2, fill = "primary" } },
        ui.Text { id = "battery-text", text = "86%", width = "content", height = 20, font_size = 13, color = "onSurfaceVariant" } },
}
