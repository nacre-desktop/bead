-- SPDX-License-Identifier: MIT
return {
    stores = {},
    actions = {},
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
