-- SPDX-License-Identifier: MIT
local ui = require("bead.ui")
local muted = ui.resolve_for_check(ui.bind("desktop.audio.muted"))
local volume = ui.resolve_for_check(ui.bind("desktop.audio.volume"))
local audio_available = ui.resolve_for_check(ui.bind("desktop.audio.available"))
local media_available = ui.resolve_for_check(ui.bind("desktop.media.available"))
local media_can_play_pause = ui.resolve_for_check(ui.bind("desktop.media.can_play_pause"))
local power_available = ui.resolve_for_check(ui.bind("desktop.power.available"))
return ui.Column {
    id = "hello-status", width = "fill", height = "fill", padding = 6, gap = 4, fill = "surface",
    ui.Row { height = 30, align = "center", gap = 8,
        ui.Repeat { id = "workspaces", model = ui.bind("status.workspaces"), key = "id", render = function(workspace)
            return ui.Button { id = "workspace", text = workspace.label, width = "content", height = 30,
                padding = 5, radius = 5, fill = workspace.fill, color = workspace.color,
                on_activate = ui.action("desktop.workspace.focus", workspace.id) }
        end },
        ui.Text { text = ui.bind("desktop.window.title"), width = 0, grow = 1, height = 24, color = "onSurface" },
    },
    ui.Row { height = 30, align = "center", gap = 16,
        ui.Text { text = ui.bind("desktop.clock"), width = "content", font_size = 16, color = "primary" },
        ui.Text { text = ui.bind("status.battery"), width = "content", font_size = 14, color = "onSurfaceVariant" },
        ui.Text { text = ui.bind("status.volume"), width = "content", font_size = 14, color = "onSurfaceVariant" },
        ui.Button { id = "mute-toggle", text = ui.bind("status.mute"), width = 72, on_activate = audio_available and ui.action("desktop.mute", not muted) or nil, height = 30, padding = 5, font_size = 13, fill = "primary", color = "onPrimary", radius = 5 },
        ui.Button { id = "volume-down", text = "−", on_activate = audio_available and ui.action("desktop.volume", math.max(0, volume - 0.1)) or nil, width = 30, height = 30, font_size = 16, fill = "surface", color = "onSurface", radius = 5 },
        ui.Button { id = "volume-up", text = "+", on_activate = audio_available and ui.action("desktop.volume", math.min(1, volume + 0.1)) or nil, width = 30, height = 30, font_size = 16, fill = "surface", color = "onSurface", radius = 5 },
        ui.Text { text = ui.bind("status.kernel"), width = "content", font_size = 14, color = "onSurface" },
        ui.Text { text = ui.bind("status.media"), width = "content", font_size = 14, color = "onSurfaceVariant" },
        ui.Button { id = "media-play-pause", text = ui.bind("status.media_button"), width = 58, on_activate = media_available and media_can_play_pause and ui.action("desktop.media.play_pause") or nil, height = 30, padding = 5, font_size = 13, fill = "primary", color = "onPrimary", radius = 5 },
        ui.Button { id = "power-profile", text = ui.bind("status.power"), width = 130, on_activate = power_available and ui.action("desktop.power.cycle") or nil, height = 30, padding = 5, font_size = 13, fill = "surface", color = "onSurface", radius = 5 },
    },
}
