-- SPDX-License-Identifier: MIT
return {
    stores = {
        ["desktop.clock"] = "",
        ["desktop.battery.percent"] = -1,
        ["desktop.battery.available"] = false,
        ["desktop.audio.available"] = false,
        ["desktop.audio.volume"] = -1,
        ["desktop.audio.muted"] = false,
        ["desktop.media.available"] = false,
        ["desktop.media.player"] = "",
        ["desktop.media.status"] = "",
        ["desktop.media.title"] = "",
        ["desktop.media.artist"] = "",
        ["desktop.media.album"] = "",
        ["desktop.media.length_ms"] = -1,
        ["desktop.media.position_ms"] = -1,
        ["desktop.media.art_url"] = "",
        ["desktop.media.can_play_pause"] = false,
        ["desktop.media.can_next"] = false,
        ["desktop.media.can_previous"] = false,
        ["desktop.media.can_seek"] = false,
        ["commands.kernel.stdout"] = "",
        ["commands.kernel.ok"] = false,
        ["commands.kernel.error"] = "",
    },
    actions = {},
    computed = {
        ["status.battery"] = function(s)
            if s["desktop.battery.available"] then
                return "Battery " .. tostring(s["desktop.battery.percent"]) .. "%"
            end
            return "Battery unavailable"
        end,
        ["status.volume"] = function(s)
            if s["desktop.audio.available"] then
                return "Volume " .. tostring(s["desktop.audio.volume"])
            end
            return "Audio unavailable"
        end,
        ["status.mute"] = function(s)
            return s["desktop.audio.muted"] and "Unmute" or "Mute"
        end,
        ["status.kernel"] = function(s)
            return "Kernel " .. s["commands.kernel.stdout"]
        end,
        ["status.media"] = function(s)
            if not s["desktop.media.available"] then return "Nothing playing" end
            local title = s["desktop.media.title"]
            if title == "" then title = s["desktop.media.player"] end
            if s["desktop.media.artist"] ~= "" then title = title .. " — " .. s["desktop.media.artist"] end
            return title
        end,
        ["status.media_button"] = function(s)
            return s["desktop.media.status"] == "Playing" and "Pause" or "Play"
        end,
    },
}
