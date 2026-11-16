-- SPDX-License-Identifier: MIT
-- The dashboard panel: calendar, weather, gauges, media card, quick toggles and sliders, all built from
-- bead.kit and tokens. Every number and name below is demo data, not a live reading. The panel slides in
-- from the bar edge (`slide_distance`) and fades; the body is a translucent card over whatever is behind it.
local ui = require("bead.ui")
local kit = require("bead.kit")
local m = ui.motion
local icon = kit.tokens.fonts.icon

local function text(id, t, size, color, extra)
    local p = { id = id, text = t, height = math.ceil(size * 1.4), font_size = size, color = color or "onSurface" }
    for k, v in pairs(extra or {}) do p[k] = v end
    return ui.Text(p)
end
local function card(ctor, p) -- the translucent tile every section sits on
    p.radius, p.fill = p.radius or 18, ui.alpha("surfaceContainer", .65)
    return ctor(p)
end

-- November 2026 starts on a Sunday; weeks run Monday to Sunday and the 16th is "today" (demo date).
local function calendar()
    local days = { id = "cal-days", height = 20 }
    for i, d in ipairs({ "M", "T", "W", "T", "F", "S", "S" }) do
        days[i] = text("cal-h" .. i, d, 12, "onSurfaceVariant", { width = 36, text_align = "center" })
    end
    local rows = { text("cal-title", "November 2026", 16, "onSurface"), ui.Row(days) }
    for week = 0, 5 do
        local row = { id = "week" .. week, height = 28 }
        for col = 1, 7 do
            local day = week * 7 + col - 6
            local shown = day >= 1 and day <= 30
            local today = day == 16
            row[col] = ui.Row { id = "d" .. week .. col, width = 36, height = 28, radius = 14, align = "center", justify = "center",
                fill = today and "primary" or "transparent",
                text("n" .. week .. col, shown and tostring(day) or "", 13, today and "onPrimary" or "onSurface",
                    { width = 36, text_align = "center" }) }
        end
        table.insert(rows, ui.Row(row))
    end
    rows.id, rows.width, rows.height, rows.padding, rows.gap = "calendar", 288, 250, 14, 2
    return card(ui.Column, rows)
end

local function weather()
    return card(ui.Row, { id = "weather", width = 288, height = 104, padding = 14, align = "center", gap = 10,
        ui.Column { id = "temp", width = 100, gap = 2,
            text("temp-now", "12°", 38, "onSurface", { height = 46 }),
            text("temp-sky", "Light rain", 14, "onSurfaceVariant") },
        ui.Column { id = "trend", grow = 1, width = 0, gap = 4, align = "end",
            ui.Text { id = "sky", text = "rainy", family = icon, font_size = 30, width = 40, height = 34, color = "primary" },
            ui.Polyline { id = "trend-line", width = 96, height = 30, stroke_width = 2, color = "primary",
                points = { { 0, .7 }, { .17, .55 }, { .34, .62 }, { .5, .3 }, { .67, .42 }, { .83, .18 }, { 1, .25 } } },
            text("trend-cap", "Next 12 h (demo)", 12, "onSurfaceVariant", { width = 120, text_align = "right" }) } })
end

local function gauge(id, label, value, shown, color)
    return card(ui.Box, { id = id, width = 88, height = 84,
        ui.Arc { id = id .. "-track", position = "absolute", left = 16, top = 6, width = 56, height = 56,
            start_angle = 135, sweep_angle = 270, stroke_width = 6, color = "surfaceContainerHighest" },
        ui.Arc { id = id .. "-value", position = "absolute", left = 16, top = 6, width = 56, height = 56,
            start_angle = 135, sweep_angle = 270 * value, stroke_width = 6, color = color },
        text(id .. "-text", shown, 14, "onSurface", { position = "absolute", left = 16, top = 23, width = 56, text_align = "center" }),
        text(id .. "-label", label, 12, "onSurfaceVariant", { position = "absolute", left = 0, top = 62, width = 88, text_align = "center" }) })
end

local function media()
    return card(ui.Column, { id = "media", width = 298, height = 180, padding = 14, gap = 8, radius = 24,
        ui.Row { id = "media-top", height = 72, gap = 12,
            ui.Image { id = "art", path = "view/art.png", width = 72, height = 72, radius = 16 },
            ui.Column { id = "media-text", grow = 1, width = 0, gap = 2, justify = "center",
                text("track", "Slow Light", 17, "onSurface"),
                text("artist", "Demo Artist", 13, "onSurfaceVariant"),
                kit.Tag { id = "demo-tag", text = "demo track" } } },
        ui.Column { id = "progress", height = 24, gap = 4,
            kit.LevelMeter { id = "bar", value = .38, width = 270, height = 6 },
            ui.Row { id = "times", height = 14,
                text("t0", "1:24", 11, "onSurfaceVariant", { width = 0, grow = 1, height = 14 }),
                text("t1", "3:41", 11, "onSurfaceVariant", { width = 40, height = 14, text_align = "right" }) } },
        ui.Row { id = "controls", height = 40, gap = 14, justify = "center", align = "center",
            kit.IconButton { id = "prev", text = "skip_previous", width = 36, height = 36, radius = 18, fill = "transparent", on_activate = ui.action("noop") },
            kit.IconButton { id = "pause", text = "pause", width = 40, height = 40, radius = 20, fill = "primary", color = "onPrimary", on_activate = ui.action("noop") },
            kit.IconButton { id = "next", text = "skip_next", width = 36, height = 36, radius = 18, fill = "transparent", on_activate = ui.action("noop") } } })
end

local function toggle(id, label, detail, glyph, store)
    return kit.ToggleTile { id = id, width = 143, height = 60, label = label, detail = detail, icon = glyph,
        on = ui.bind(store), on_activate = ui.action("toggle_" .. store) }
end

local body = ui.Column {
    id = "body", position = "absolute", top = 10, left = 32, width = 640, height = 560, radius = 28, shadow = true,
    fill = ui.alpha("surface", .68), padding = 20, gap = 14,
    ui.Row { id = "header", height = 40, align = "center",
        ui.Column { id = "greeting", grow = 1, width = 0,
            text("hello", ui.bind("greeting"), 22, "onSurface", { height = 28 }),
            text("sub", "Monday 16 November - demo data", 12, "onSurfaceVariant", { height = 16 }) },
        kit.IconButton { id = "settings", text = "settings", width = 36, height = 36, radius = 18, fill = "transparent",
            on_activate = ui.action("noop") } },
    ui.Row { id = "main", height = 466, gap = 14,
        ui.Column { id = "left", width = 288, gap = 14, calendar(), weather(),
            ui.Row { id = "gauges", height = 84, gap = 12,
                gauge("cpu", "CPU", .23, "23%", "primary"), gauge("mem", "Memory", .41, "41%", "secondary"),
                gauge("temp", "Temp", .52, "52°", "error") } },
        ui.Column { id = "right", width = 298, gap = 14, media(),
            ui.Row { id = "toggles-a", height = 60, gap = 12,
                toggle("wifi", "Wi-Fi", "Demo network", "wifi", "wifi"), toggle("bluetooth", "Bluetooth", "2 devices", "bluetooth", "bluetooth") },
            ui.Row { id = "toggles-b", height = 60, gap = 12,
                toggle("night", "Night light", "Off", "dark_mode", "night"), toggle("dnd", "Quiet", "Off", "notifications_off", "dnd") },
            kit.SliderTile { id = "volume", width = 298, height = 56, label = "Volume", icon = "volume_up", detail = "62%",
                value = ui.bind("volume"), on_change = "set_volume", on_toggle = ui.action("noop") },
            kit.SliderTile { id = "mic", width = 298, height = 56, label = "Microphone", icon = "mic", detail = "40%",
                value = ui.bind("mic"), on_change = "set_mic", on_toggle = ui.action("noop") } } },
}

return ui.Panel {
    id = "dashboard", width = "fill", height = "fill", fill = "transparent", shadow = false, body = "body",
    open = true, reveal = false, slide_distance = 570, edge = "top", fade = true, duration = m.duration.open,
    body,
}
