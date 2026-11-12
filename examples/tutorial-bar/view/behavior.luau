-- SPDX-License-Identifier: MIT
return {
    stores = {
        ["desktop.clock"] = "",
        ["desktop.battery.available"] = false,
        ["desktop.battery.percent"] = -1,
        ["desktop.audio.available"] = false,
        ["desktop.audio.volume"] = -1,
        ["desktop.audio.muted"] = false,
    },
    actions = { ["desktop.mute"] = function(_, _) end },
    computed = {
        battery = function(s)
            return s["desktop.battery.available"] and ("Battery " .. tostring(s["desktop.battery.percent"]) .. "%") or "Battery unavailable"
        end,
        volume = function(s)
            return s["desktop.audio.available"] and ("Volume " .. tostring(s["desktop.audio.volume"])) or "Audio unavailable"
        end,
        mute = function(s) return s["desktop.audio.muted"] and "Unmute" or "Mute" end,
    },
}
