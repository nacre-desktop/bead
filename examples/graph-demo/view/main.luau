-- SPDX-License-Identifier: MIT
local ui = require("bead.ui")
local kit = require("bead.kit")
return ui.Column {
    id = "graph-demo", width = "fill", height = "fill", padding = 12, gap = 8, fill = "surface",
    kit.GraphCanvas { id = "canvas", grow = 1, graph = require("./graph"), fit = ui.bind("fit"),
        style = { groups = { "primary", "secondary", "primaryContainer" } },
        fill = ui.alpha("onSurface", 0.04),
        highlight = ui.bind("highlight"),
        on_hover = ui.action("hover"), on_select = ui.action("select"), on_activate = ui.action("activate") },
    ui.Row { id = "bar", width = "fill", height = 30, gap = 8,
        ui.Text { id = "status", text = ui.bind("status"), grow = 1, height = 24, font_size = 14, color = "onSurfaceVariant" },
        ui.Button { id = "refit", text = "Fit", width = 56, height = 30, padding = 4, font_size = 14, color = "onPrimary", fill = "primary", on_activate = ui.action("refit") } },
}
