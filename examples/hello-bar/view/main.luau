-- SPDX-License-Identifier: MIT
local ui = require("bead.ui")
return ui.Row {
    id = "hello-bar", width = "fill", height = "fill", padding = 12, gap = 0,
    fill = "surface",
    ui.Text { text = "Hello bar", font_size = 15, height = "fill", color = "primary", width = "fill" },
    ui.Text { text = "12:34", font_size = 14, height = "fill", color = "onSurfaceVariant" },
}
