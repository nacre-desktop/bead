-- SPDX-License-Identifier: MIT
local ui = require("bead.ui")
local muted = ui.resolve_for_check(ui.bind("desktop.audio.muted"))
local volume = ui.resolve_for_check(ui.bind("desktop.audio.volume"))
local audio_available = ui.resolve_for_check(ui.bind("desktop.audio.available"))
local media_available = ui.resolve_for_check(ui.bind("desktop.media.available"))
local media_can_play_pause = ui.resolve_for_check(ui.bind("desktop.media.can_play_pause"))
return ui.Row {
    id = "hello-status", width = "fill", height = "fill", align = "center",
    padding_left = 12, padding_right = 12, gap = 16, fill = "surface",
    ui.Text { text = ui.bind("desktop.clock"), width = "content", font_size = 16, color = "primary" },
    ui.Text { text = ui.bind("status.battery"), width = "content", font_size = 14, color = "onSurfaceVariant" },
    ui.Text { text = ui.bind("status.volume"), width = "content", font_size = 14, color = "onSurfaceVariant" },
    ui.Button { id = "mute-toggle", text = ui.bind("status.mute"), width = 72, on_activate = audio_available and ui.action("desktop.mute", not muted) or nil, height = 30, padding = 5, font_size = 13, fill = "primary", color = "onPrimary", radius = 5 },
    ui.Button { id = "volume-down", text = "−", on_activate = audio_available and ui.action("desktop.volume", math.max(0, volume - 0.1)) or nil, width = 30, height = 30, font_size = 16, fill = "surface", color = "onSurface", radius = 5 },
    ui.Button { id = "volume-up", text = "+", on_activate = audio_available and ui.action("desktop.volume", math.min(1, volume + 0.1)) or nil, width = 30, height = 30, font_size = 16, fill = "surface", color = "onSurface", radius = 5 },
    ui.Text { text = ui.bind("status.kernel"), width = "content", font_size = 14, color = "onSurface" },
    ui.Text { text = ui.bind("status.media"), width = "content", font_size = 14, color = "onSurfaceVariant" },
    ui.Button { id = "media-play-pause", text = ui.bind("status.media_button"), width = 58, on_activate = media_available and media_can_play_pause and ui.action("desktop.media.play_pause") or nil, height = 30, padding = 5, font_size = 13, fill = "primary", color = "onPrimary", radius = 5 },
}
