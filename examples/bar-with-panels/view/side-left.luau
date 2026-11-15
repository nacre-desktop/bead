-- SPDX-License-Identifier: MIT
-- A side dock opened by the left lip: the body grows out of the left screen edge (reveal) and the
-- lip rides the body's inner edge as the panel's tab while it is open (spec 0279). The root keeps a
-- 32 px shadow margin on the inner side only, so the body is flush with the screen edge.
local ui = require("bead.ui")
local m = ui.motion
return ui.Panel {
    id = "side-left", width = "fill", height = "fill", fill = "transparent", shadow = false, body = "body",
    open = true, reveal = true, edge = "left", fade = false, duration = m.duration.open,
    ui.Column {
        id = "body", position = "absolute", top = 32, left = 0, width = 268, height = 436,
        shadow = true, fill = "surface", radius = 16, join_edge = "left", padding = 16, gap = 8,
        ui.Text { text = "Side dock (left)", font_size = 18, height = 26, color = "primary" },
        ui.Text { text = ui.bind("desktop.clock"), font_size = 14, height = 20, color = "onSurfaceVariant" },
        ui.Button { id = "close", text = "Close", width = 60, height = 28, padding = 4, font_size = 13, radius = 5,
            fill = "surface", color = "onSurfaceVariant", on_activate = ui.action("panel.close") },
    },
}
