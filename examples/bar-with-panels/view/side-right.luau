-- SPDX-License-Identifier: MIT
-- A side dock opened by the right lip: the body slides in from the right screen edge (slide_distance =
-- its width plus the inset) and the lip rides the body's inner edge as the panel's tab while it is
-- open (spec 0279). Inset 10 px from the screen edge like a framed host dock: the tab still starts on
-- the free lip and settles flush on the body (B0289). The root keeps a 32 px shadow margin inside.
local ui = require("bead.ui")
local m = ui.motion
return ui.Panel {
    id = "side-right", width = "fill", height = "fill", fill = "transparent", shadow = false, body = "body",
    open = true, reveal = false, slide_distance = 268, edge = "right", fade = false, duration = m.duration.open,
    ui.Column {
        id = "body", position = "absolute", top = 32, left = 32, width = 258, height = 436,
        shadow = true, fill = "surface", radius = 16, join_edge = "right", padding = 16, gap = 8,
        ui.Text { text = "Side dock (right)", font_size = 18, height = 26, color = "primary" },
        ui.Text { text = ui.bind("desktop.clock"), font_size = 14, height = 20, color = "onSurfaceVariant" },
        ui.Button { id = "close", text = "Close", width = 60, height = 28, padding = 4, font_size = 13, radius = 5,
            fill = "surface", color = "onSurfaceVariant", on_activate = ui.action("panel.close") },
    },
}
