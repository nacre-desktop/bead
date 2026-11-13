-- SPDX-License-Identifier: MIT
local ui = require("bead.ui")
local kit = require("bead.kit")
return ui.Column {
    id = "graph-demo", width = "fill", height = "fill", padding = 12, gap = 8, fill = "surface",
    kit.GraphCanvas { id = "canvas", grow = 1, graph = require("./graph"), fill = ui.alpha("onSurface", 0.04),
        highlight = ui.bind("highlight"),
        on_hover = ui.action("hover"), on_select = ui.action("select"), on_activate = ui.action("activate") },
    ui.Text { id = "status", text = ui.bind("status"), height = 24, font_size = 14, color = "onSurfaceVariant" },
}
