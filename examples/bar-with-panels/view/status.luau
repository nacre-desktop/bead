-- SPDX-License-Identifier: MIT
-- The status peek: a small body under the bar that grows into a panel on "More" and back on
-- "Less", as one surface. The root stays at the declared maximum size; the body animates with
-- ui.animated markers (no Luau per frame), joined to the bar edge, and the reveal clips to it.
local ui = require("bead.ui")
local m = ui.motion
local grown = ui.resolve_for_check(ui.bind("grown"))
local width, height = grown and 424 or 300, grown and 300 or 120
return ui.Panel {
    id = "status", width = "fill", height = "fill", fill = "transparent", shadow = false, body = "body",
    open = true, reveal = true, edge = "top", fade = false, duration = m.duration.peek,
    ui.Box {
        id = "body", position = "absolute", top = 0, shadow = true, fill = "surface", radius = 16, join_edge = "top",
        left = ui.animated("status.x", (424 - width) / 2, m.duration.open, m.curve.out_cubic),
        width = ui.animated("status.w", width, m.duration.open, m.curve.out_cubic),
        height = ui.animated("status.h", height, m.duration.open, m.curve.out_cubic),
        padding = 16, gap = 8,
        ui.Text { text = ui.bind("desktop.clock"), font_size = 18, height = 26, color = "primary" },
        ui.Text { text = ui.bind("volume"), font_size = 14, height = 20,
            color = ui.alpha("onSurfaceVariant", ui.animated("status.detail", grown and 1 or 0.7, m.duration.fade, "out-cubic")) },
        ui.Row {
            id = "actions", gap = 8,
            ui.Button { id = "grow", text = grown and "Less" or "More", width = 72, height = 28, padding = 4, font_size = 13, radius = 5,
                fill = ui.mix("surfaceContainer", "primary", ui.animated("status.grow", grown and 1 or 0, m.duration.switch, "out-cubic")),
                color = "onSurface", on_activate = ui.action("grow") },
            ui.Button { id = "close", text = "Close", width = 60, height = 28, padding = 4, font_size = 13, radius = 5,
                fill = "surface", color = "onSurfaceVariant", on_activate = ui.action("panel.close") },
        },
    },
}
