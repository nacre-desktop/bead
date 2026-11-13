-- SPDX-License-Identifier: MIT
-- One of each kit component added in spec 0274, on a dark palette.
local ui = require("bead.ui")
local kit = require("bead.kit")
local function caption(text) return ui.Text { text = text, height = 18, font_size = 12, color = "onSurfaceVariant" } end
return ui.Column {
    id = "gallery", width = "fill", height = "fill", padding = 20, gap = 10, fill = "surface",
    caption("Segment"),
    kit.Segment { id = "mode", aria_label = "Mode", value = ui.bind("mode"), on_change = "set_mode",
        options = { { id = "light", label = "Light" }, { id = "dark", label = "Dark" }, { id = "auto", label = "Auto" } } },
    caption("Chip and Tag"),
    ui.Row { id = "chips", height = 36, gap = 8, align = "center",
        kit.Chip { id = "wifi", text = "Wi-Fi", icon = "wifi", on = ui.bind("wifi"), on_activate = ui.action("toggle_wifi") },
        kit.Chip { id = "quiet", text = "Quiet", on = false, on_activate = ui.action("noop") },
        kit.Chip { id = "folder", text = "~/projects", icon = "folder", on = false, on_activate = ui.action("noop") },
        kit.Tag { id = "tag", text = "codex" } },
    caption("LinkRow"),
    kit.LinkRow { id = "link", title = "Open sidebar", subtitle = "Pick the edge and the width", on_activate = ui.action("noop") },
    kit.LinkRow { id = "link-icon", title = "Network", icon = "wifi", on_activate = ui.action("noop") },
    caption("SliderTile with an end slot"),
    kit.SliderTile { id = "volume", width = 380, height = 56, label = "Volume", icon = "volume_up", detail = "60%",
        value = ui.bind("volume"), on_change = "set_volume", on_toggle = ui.action("noop"),
        end_icon = "mic", end_label = "Mute microphone", on_end = ui.action("toggle_muted") },
    caption("ToggleTile"),
    kit.ToggleTile { id = "tile", width = 180, height = 64, label = "Wi-Fi", detail = "Home", icon = "wifi", on = true,
        on_activate = ui.action("noop") },
}
