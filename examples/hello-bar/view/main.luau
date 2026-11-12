-- SPDX-License-Identifier: MIT
local ui = require("bead.ui")
return ui.Row {
    id = "hello-bar", width = "fill", height = "fill", align = "center", padding_left = 12, padding_right = 12, gap = 0,
    fill = "surface",
    ui.Text { text = "Hello bar", font_size = 15, color = "primary", width = 0, grow = 1 },
    ui.Text { text = "12:34", font_size = 14, color = "onSurfaceVariant", width = 64 },
}
