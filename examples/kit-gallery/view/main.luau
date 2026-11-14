-- SPDX-License-Identifier: MIT
-- One of each kit component added in specs 0274, 0276 and 0280, on a dark palette.
local ui = require("bead.ui")
local kit = require("bead.kit")
local function caption(text) return ui.Text { text = text, height = 18, font_size = 12, color = "onSurfaceVariant" } end
-- Spec 0280: a shadowed card, hover-only dismiss buttons and a toast stack that slides when a toast
-- comes or goes (`reflow`); every kit button presses to .97 with a deeper fill.
local batch3 = ui.Column { id = "batch3", width = 400, gap = 10,
    caption("Card with a shadow"),
    kit.Card { id = "card", width = 380, height = 56, shadow = true,
        ui.Text { id = "card-text", text = "Same shadow as panels and peeks", height = 20, font_size = 14, color = "onSurface" } },
    caption("Toast stack: hover a toast for its dismiss button"),
    ui.Row { id = "stack-actions", height = 36, gap = 8, align = "center",
        kit.Chip { id = "add", text = "Add toast", icon = "add", on = false, on_activate = ui.action("add_toast") } },
    ui.Column { id = "stack", width = 380, gap = 8, reflow = ui.motion.duration.switch,
        ui.Repeat { id = "toasts", model = ui.bind("toasts"), key = "id", render = function(item)
            return kit.NoticeRow { id = item.id, width = 380, caption = "Mail · now", title = item.title, body = "Lunch at noon?",
                on_dismiss = ui.action("drop_toast", item.id) }
        end } },
}
return ui.Row { id = "page", width = "fill", height = "fill", fill = "surface",
  ui.Column {
    id = "gallery", width = 420, height = "fill", padding = 20, gap = 10, fill = "surface",
    caption("Segment"),
    kit.Segment { id = "mode", aria_label = "Mode", value = ui.bind("mode"), on_change = "set_mode",
        options = { { id = "light", label = "Light" }, { id = "dark", label = "Dark" }, { id = "auto", label = "Auto" } } },
    caption("Chip and Tag"),
    ui.Row { id = "chips", height = 36, gap = 8, align = "center",
        kit.Chip { id = "wifi", text = "Wi-Fi", icon = "wifi", on = ui.bind("wifi"), on_activate = ui.action("toggle_wifi") },
        kit.Chip { id = "quiet", text = "Quiet", on = false, on_activate = ui.action("noop") },
        kit.Chip { id = "folder", text = "~/projects", icon = "folder", on = false, on_activate = ui.action("noop") },
        kit.Tag { id = "tag", text = "codex" } },
    ui.Row { id = "buttons", height = 36, gap = 8, align = "center",
        kit.PillButton { id = "pill", label = "Open Brain", glyph = "psychology", on_activate = ui.action("noop") },
        kit.ConfirmButton { id = "confirm", width = 150, height = 36, label = "Clear history", glyph = "delete", on_activate = ui.action("noop"), on_confirm = ui.action("noop") } },
    caption("LinkRow"),
    kit.LinkRow { id = "link", title = "Open sidebar", subtitle = "Pick the edge and the width", on_activate = ui.action("noop") },
    kit.LinkRow { id = "link-icon", title = "Network", icon = "wifi", on_activate = ui.action("noop") },
    caption("SliderTile with an end slot"),
    kit.SliderTile { id = "volume", width = 380, height = 56, label = "Volume", icon = "volume_up", detail = "60%",
        value = ui.bind("volume"), on_change = "set_volume", on_toggle = ui.action("noop"),
        end_icon = "mic", end_label = "Mute microphone", on_end = ui.action("toggle_muted") },
    caption("EmptyState, NoticeRow and a rounded image"),
    ui.Row { id = "batch2", height = 150, gap = 10,
        kit.EmptyState { id = "empty", width = 160, height = 150, glyph = "inbox", title = "All clear", detail = "No notifications", chip = "Open Brain", chip_glyph = "psychology", on_chip = ui.action("noop") },
        ui.Box { id = "art", width = 96, height = 96, radius = 24, fill = "surfaceContainerHigh",
            ui.Image { id = "art-image", path = "view/art.svg", width = 96, height = 96, radius = 24 } } },
    kit.NoticeRow { id = "note", width = 380, caption = "Mail · now", title = "New message", body = "Lunch at noon?", on_dismiss = ui.action("noop") },
    caption("ToggleTile"),
    kit.ToggleTile { id = "tile", width = 180, height = 64, label = "Wi-Fi", detail = "Home", icon = "wifi", on = true,
        on_activate = ui.action("noop") },
  },
  ui.Column { id = "right", width = 440, height = "fill", padding = 20, batch3 },
}
